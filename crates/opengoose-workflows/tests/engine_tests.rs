use opengoose_workflows::*;

fn step(id: &str, name: &str, prompt: &str, depends_on: Vec<&str>) -> StepDef {
    StepDef {
        id: id.into(),
        name: name.into(),
        agent: "bot".into(),
        prompt: prompt.into(),
        expects: vec![],
        max_retries: 2,
        depends_on: depends_on.into_iter().map(String::from).collect(),
        on_fail: OnFailStrategy::Abort,
        loop_config: None,
    }
}

fn simple_def() -> WorkflowDef {
    WorkflowDef {
        name: "test".into(),
        description: "test workflow".into(),
        agents: vec![AgentDef {
            id: "bot".into(),
            name: "Bot".into(),
            system_prompt: "you are a bot".into(),
        }],
        steps: vec![
            step("step1", "First", "Do {{input}}", vec![]),
            {
                let mut s = step("step2", "Second", "Continue from {{step1}}", vec!["step1"]);
                s.expects = vec!["Output is valid".into()];
                s
            },
        ],
    }
}

#[test]
fn engine_resolves_input_placeholder() {
    let engine = WorkflowEngine::new(simple_def(), "hello world".into());
    let prompt = engine.current_prompt().unwrap().unwrap();
    assert!(prompt.contains("Do hello world"));
    assert!(!prompt.contains("{{input}}"));
}

#[test]
fn engine_resolves_dependency_output() {
    let mut engine = WorkflowEngine::new(simple_def(), "task".into());
    engine.mark_running();
    engine.record_outcome(StepOutcome::Completed {
        output: "step1 result".into(),
    });
    let prompt = engine.current_prompt().unwrap().unwrap();
    assert!(prompt.contains("Continue from step1 result"));
    assert!(!prompt.contains("{{step1}}"));
}

#[test]
fn engine_appends_expects_to_prompt() {
    let mut engine = WorkflowEngine::new(simple_def(), "task".into());
    engine.mark_running();
    engine.record_outcome(StepOutcome::Completed {
        output: "done".into(),
    });
    let prompt = engine.current_prompt().unwrap().unwrap();
    assert!(prompt.contains("Acceptance Criteria"));
    assert!(prompt.contains("Output is valid"));
}

#[test]
fn engine_errors_on_unsatisfied_dependency() {
    let def = WorkflowDef {
        name: "test".into(),
        description: String::new(),
        agents: vec![AgentDef {
            id: "bot".into(),
            name: "Bot".into(),
            system_prompt: "".into(),
        }],
        steps: vec![
            {
                let mut s = step("a", "A", "do A", vec![]);
                s.max_retries = 0;
                s
            },
            {
                let mut s = step("b", "B", "use {{a}}", vec!["a"]);
                s.max_retries = 0;
                s
            },
        ],
    };

    let mut engine = WorkflowEngine::new(def, "input".into());
    engine.mark_running();
    engine.record_outcome(StepOutcome::Failed {
        reason: "broken".into(),
    });
    assert!(engine.state().is_failed());
}

#[test]
fn engine_step_context_has_correct_fields() {
    let engine = WorkflowEngine::new(simple_def(), "my task".into());
    let ctx = engine.current_step_context().unwrap().unwrap();
    assert_eq!(ctx.step_id, "step1");
    assert_eq!(ctx.step_name, "First");
    assert_eq!(ctx.agent_id, "bot");
    assert_eq!(ctx.agent_name, "Bot");
    assert_eq!(ctx.system_prompt, "you are a bot");
    assert!(ctx.user_prompt.contains("Do my task"));
    assert_eq!(ctx.progress, (0, 2));
    assert!(ctx.loop_iteration.is_none());
}

#[test]
fn engine_retry_then_succeed() {
    let mut engine = WorkflowEngine::new(simple_def(), "task".into());
    engine.mark_running();
    let has_more = engine.record_outcome(StepOutcome::Retry {
        reason: "not good".into(),
    });
    assert!(has_more);
    assert_eq!(engine.state().steps[0].retries, 1);
    assert_eq!(engine.state().steps[0].status, StepStatus::Pending);

    engine.mark_running();
    let has_more = engine.record_outcome(StepOutcome::Completed {
        output: "ok".into(),
    });
    assert!(has_more);
    assert_eq!(engine.state().steps[0].status, StepStatus::Completed);
    assert_eq!(engine.progress(), (1, 2));
}

#[test]
fn engine_retry_exhaustion_aborts() {
    let mut engine = WorkflowEngine::new(simple_def(), "task".into());
    engine.mark_running();
    engine.record_outcome(StepOutcome::Retry {
        reason: "fail 1".into(),
    });
    engine.mark_running();
    let has_more = engine.record_outcome(StepOutcome::Retry {
        reason: "fail 2".into(),
    });
    assert!(!has_more);
    assert_eq!(engine.state().steps[0].status, StepStatus::Failed);
    assert!(engine.state().is_failed());
}

#[test]
fn engine_on_fail_skip_continues() {
    let def = WorkflowDef {
        name: "test".into(),
        description: String::new(),
        agents: vec![AgentDef {
            id: "bot".into(),
            name: "Bot".into(),
            system_prompt: "".into(),
        }],
        steps: vec![
            {
                let mut s = step("optional", "Optional", "try this", vec![]);
                s.max_retries = 0;
                s.on_fail = OnFailStrategy::Skip;
                s
            },
            step("required", "Required", "do this", vec![]),
        ],
    };

    let mut engine = WorkflowEngine::new(def, "input".into());
    engine.mark_running();
    let has_more = engine.record_outcome(StepOutcome::Failed {
        reason: "not critical".into(),
    });
    assert!(has_more);
    assert_eq!(engine.state().steps[0].status, StepStatus::Skipped);
    assert_eq!(engine.state().current_step, 1);
}

#[test]
fn engine_completion_detection() {
    let def = WorkflowDef {
        name: "test".into(),
        description: String::new(),
        agents: vec![AgentDef {
            id: "bot".into(),
            name: "Bot".into(),
            system_prompt: "".into(),
        }],
        steps: vec![step("only", "Only", "{{input}}", vec![])],
    };

    let mut engine = WorkflowEngine::new(def, "go".into());
    engine.mark_running();
    let has_more = engine.record_outcome(StepOutcome::Completed {
        output: "done".into(),
    });
    assert!(!has_more);
    assert!(engine.state().is_complete());
    assert!(!engine.state().is_failed());
    assert_eq!(engine.progress(), (1, 1));
}

// --- New tests for shared context ---

#[test]
fn engine_extracts_context_from_output() {
    let mut engine = WorkflowEngine::new(simple_def(), "task".into());
    engine.mark_running();
    engine.record_outcome(StepOutcome::Completed {
        output: "Here is the result.\nSTATUS: done\nREPO: /tmp/myrepo\nCOUNT: 5".into(),
    });
    assert_eq!(engine.state().context.get("status"), Some(&"done".into()));
    assert_eq!(
        engine.state().context.get("repo"),
        Some(&"/tmp/myrepo".into())
    );
    assert_eq!(engine.state().context.get("count"), Some(&"5".into()));
}

#[test]
fn engine_resolves_context_placeholders() {
    let def = WorkflowDef {
        name: "test".into(),
        description: String::new(),
        agents: vec![AgentDef {
            id: "bot".into(),
            name: "Bot".into(),
            system_prompt: "".into(),
        }],
        steps: vec![
            step("s1", "S1", "{{input}}", vec![]),
            step("s2", "S2", "Repo is {{repo}}, branch is {{branch}}", vec!["s1"]),
        ],
    };

    let mut engine = WorkflowEngine::new(def, "go".into());
    engine.mark_running();
    engine.record_outcome(StepOutcome::Completed {
        output: "Done!\nREPO: /code\nBRANCH: main".into(),
    });
    let prompt = engine.current_prompt().unwrap().unwrap();
    assert!(prompt.contains("Repo is /code"), "prompt: {prompt}");
    assert!(prompt.contains("branch is main"), "prompt: {prompt}");
}

#[test]
fn engine_context_ignores_lowercase_keys() {
    let mut engine = WorkflowEngine::new(simple_def(), "task".into());
    engine.mark_running();
    engine.record_outcome(StepOutcome::Completed {
        output: "lowercase: not extracted\nUPPER: extracted".into(),
    });
    assert!(engine.state().context.get("lowercase").is_none());
    assert_eq!(
        engine.state().context.get("upper"),
        Some(&"extracted".into())
    );
}

// --- New tests for loop steps ---

#[test]
fn engine_loop_step_iterates() {
    let def = WorkflowDef {
        name: "test".into(),
        description: String::new(),
        agents: vec![AgentDef {
            id: "bot".into(),
            name: "Bot".into(),
            system_prompt: "".into(),
        }],
        steps: vec![
            step("plan", "Plan", "{{input}}", vec![]),
            {
                let mut s = step("fix", "Fix", "Fix {{current_item}}, remaining: {{items_remaining}}", vec!["plan"]);
                s.loop_config = Some(LoopConfig {
                    over: "stories_json".into(),
                    verify_each: false,
                    verify_step: None,
                });
                s
            },
        ],
    };

    let mut engine = WorkflowEngine::new(def, "task".into());

    // Complete plan step, injecting STORIES_JSON into context
    engine.mark_running();
    engine.record_outcome(StepOutcome::Completed {
        output: "STORIES_JSON: [\"story-1\", \"story-2\", \"story-3\"]".into(),
    });

    // Now at the loop step — initialize it
    assert!(engine.is_current_loop());
    assert!(engine.init_loop().unwrap());

    // Iteration 1
    let ctx = engine.current_step_context().unwrap().unwrap();
    assert_eq!(ctx.loop_iteration, Some((0, 3)));
    assert!(ctx.user_prompt.contains("Fix story-1"));
    assert!(ctx.user_prompt.contains("remaining: 2"));

    engine.mark_running();
    let has_more = engine.record_outcome(StepOutcome::Completed {
        output: "Fixed story 1".into(),
    });
    assert!(has_more);

    // Iteration 2
    let ctx = engine.current_step_context().unwrap().unwrap();
    assert_eq!(ctx.loop_iteration, Some((1, 3)));
    assert!(ctx.user_prompt.contains("Fix story-2"));

    engine.mark_running();
    engine.record_outcome(StepOutcome::Completed {
        output: "Fixed story 2".into(),
    });

    // Iteration 3
    let ctx = engine.current_step_context().unwrap().unwrap();
    assert_eq!(ctx.loop_iteration, Some((2, 3)));

    engine.mark_running();
    let has_more = engine.record_outcome(StepOutcome::Completed {
        output: "Fixed story 3".into(),
    });

    // All iterations done — workflow complete
    assert!(!has_more);
    assert!(engine.state().is_complete());
}

// --- New tests for persistence ---

#[test]
fn state_serialization_roundtrip() {
    let mut state = WorkflowState::new("test".into(), "input".into(), vec!["s1".into(), "s2".into()]);
    state.context.insert("repo".into(), "/tmp".into());
    state.steps[0].status = StepStatus::Completed;
    state.steps[0].output = Some("output".into());

    let json = serde_json::to_string(&state).unwrap();
    let loaded: WorkflowState = serde_json::from_str(&json).unwrap();

    assert_eq!(loaded.workflow_name, "test");
    assert_eq!(loaded.context.get("repo"), Some(&"/tmp".into()));
    assert_eq!(loaded.steps[0].status, StepStatus::Completed);
    assert_eq!(loaded.steps[0].output.as_deref(), Some("output"));
}

#[test]
fn persist_store_save_and_load() {
    let dir = std::env::temp_dir().join("opengoose_test_persist");
    let _ = std::fs::remove_dir_all(&dir);
    let store = WorkflowStore::new(dir.clone()).unwrap();

    let state = WorkflowState::new("myflow".into(), "hello".into(), vec!["s1".into()]);
    store.save("run-001", &state).unwrap();

    let loaded = store.load("run-001", "myflow").unwrap();
    assert_eq!(loaded.workflow_name, "myflow");
    assert_eq!(loaded.input, "hello");

    let runs = store.list_runs("myflow").unwrap();
    assert!(runs.contains(&"run-001".into()));

    store.remove("run-001", "myflow").unwrap();
    assert!(store.load("run-001", "myflow").is_err());

    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn engine_resume_from_state() {
    let def = WorkflowDef {
        name: "test".into(),
        description: String::new(),
        agents: vec![AgentDef {
            id: "bot".into(),
            name: "Bot".into(),
            system_prompt: "".into(),
        }],
        steps: vec![
            step("s1", "S1", "{{input}}", vec![]),
            step("s2", "S2", "continue", vec![]),
        ],
    };

    // Create a state where step 1 is already done
    let mut state = WorkflowState::new("test".into(), "task".into(), vec!["s1".into(), "s2".into()]);
    state.steps[0].status = StepStatus::Completed;
    state.steps[0].output = Some("s1 output".into());
    state.current_step = 1;

    let mut engine = WorkflowEngine::resume(def, state);

    // Should be at step 2 immediately
    let ctx = engine.current_step_context().unwrap().unwrap();
    assert_eq!(ctx.step_id, "s2");

    engine.mark_running();
    let has_more = engine.record_outcome(StepOutcome::Completed {
        output: "done".into(),
    });
    assert!(!has_more);
    assert!(engine.state().is_complete());
}
