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
        timeout_seconds: None,
        when: None,
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
            profile: None,
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

// ============= Basic engine tests =============

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
            profile: None,
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
    assert!(!ctx.is_verify);
    assert!(ctx.timeout_seconds.is_none());
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
            profile: None,
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
            profile: None,
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

// ============= Shared context tests =============

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
            profile: None,
        }],
        steps: vec![
            step("s1", "S1", "{{input}}", vec![]),
            step(
                "s2",
                "S2",
                "Repo is {{repo}}, branch is {{branch}}",
                vec!["s1"],
            ),
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

#[test]
fn engine_context_ignores_digit_starting_keys() {
    let mut engine = WorkflowEngine::new(simple_def(), "task".into());
    engine.mark_running();
    engine.record_outcome(StepOutcome::Completed {
        output: "2024: timestamp\nVALID_KEY: yes".into(),
    });
    // "2024" starts with a digit, should not be extracted
    assert!(engine.state().context.get("2024").is_none());
    assert_eq!(
        engine.state().context.get("valid_key"),
        Some(&"yes".into())
    );
}

// ============= Loop step tests =============

#[test]
fn engine_loop_step_iterates() {
    let def = WorkflowDef {
        name: "test".into(),
        description: String::new(),
        agents: vec![AgentDef {
            id: "bot".into(),
            name: "Bot".into(),
            system_prompt: "".into(),
            profile: None,
        }],
        steps: vec![
            step("plan", "Plan", "{{input}}", vec![]),
            {
                let mut s = step(
                    "fix",
                    "Fix",
                    "Fix {{current_item}}, remaining: {{items_remaining}}",
                    vec!["plan"],
                );
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

    engine.mark_running();
    engine.record_outcome(StepOutcome::Completed {
        output: "STORIES_JSON: [\"story-1\", \"story-2\", \"story-3\"]".into(),
    });

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

    assert!(!has_more);
    assert!(engine.state().is_complete());
}

#[test]
fn engine_loop_accumulated_output() {
    let def = WorkflowDef {
        name: "test".into(),
        description: String::new(),
        agents: vec![AgentDef {
            id: "bot".into(),
            name: "Bot".into(),
            system_prompt: "".into(),
            profile: None,
        }],
        steps: vec![
            step("plan", "Plan", "{{input}}", vec![]),
            {
                let mut s = step("impl", "Impl", "Do {{current_item}}", vec!["plan"]);
                s.loop_config = Some(LoopConfig {
                    over: "items_json".into(),
                    verify_each: false,
                    verify_step: None,
                });
                s
            },
        ],
    };

    let mut engine = WorkflowEngine::new(def, "task".into());
    engine.mark_running();
    engine.record_outcome(StepOutcome::Completed {
        output: "ITEMS_JSON: [\"a\", \"b\"]".into(),
    });

    engine.init_loop().unwrap();

    engine.mark_running();
    engine.record_outcome(StepOutcome::Completed {
        output: "output-a".into(),
    });
    engine.mark_running();
    engine.record_outcome(StepOutcome::Completed {
        output: "output-b".into(),
    });

    // The step output should be the accumulated output of all iterations
    let output = engine.state().steps[1].output.as_deref().unwrap();
    assert!(output.contains("output-a"), "output: {output}");
    assert!(output.contains("output-b"), "output: {output}");
    assert!(output.contains("---"), "should have separator: {output}");
}

#[test]
fn engine_loop_retry_within_iteration() {
    let def = WorkflowDef {
        name: "test".into(),
        description: String::new(),
        agents: vec![AgentDef {
            id: "bot".into(),
            name: "Bot".into(),
            system_prompt: "".into(),
            profile: None,
        }],
        steps: vec![
            step("plan", "Plan", "{{input}}", vec![]),
            {
                let mut s = step("fix", "Fix", "Do {{current_item}}", vec!["plan"]);
                s.loop_config = Some(LoopConfig {
                    over: "items_json".into(),
                    verify_each: false,
                    verify_step: None,
                });
                s.max_retries = 2;
                s
            },
        ],
    };

    let mut engine = WorkflowEngine::new(def, "task".into());
    engine.mark_running();
    engine.record_outcome(StepOutcome::Completed {
        output: "ITEMS_JSON: [\"item-1\", \"item-2\"]".into(),
    });

    engine.init_loop().unwrap();

    // Item 1: first attempt fails (retry)
    engine.mark_running();
    let has_more = engine.record_outcome(StepOutcome::Retry {
        reason: "bad output".into(),
    });
    assert!(has_more);
    assert_eq!(engine.state().steps[1].retries, 1);
    assert_eq!(engine.state().steps[1].status, StepStatus::Pending);

    // Item 1: second attempt succeeds
    engine.mark_running();
    let has_more = engine.record_outcome(StepOutcome::Completed {
        output: "Fixed item 1".into(),
    });
    assert!(has_more);

    // Item 2: succeeds immediately
    engine.mark_running();
    let has_more = engine.record_outcome(StepOutcome::Completed {
        output: "Fixed item 2".into(),
    });
    assert!(!has_more);
    assert!(engine.state().is_complete());
}

#[test]
fn engine_loop_failed_in_iteration_aborts() {
    let def = WorkflowDef {
        name: "test".into(),
        description: String::new(),
        agents: vec![AgentDef {
            id: "bot".into(),
            name: "Bot".into(),
            system_prompt: "".into(),
            profile: None,
        }],
        steps: vec![
            step("plan", "Plan", "{{input}}", vec![]),
            {
                let mut s = step("fix", "Fix", "Do {{current_item}}", vec!["plan"]);
                s.loop_config = Some(LoopConfig {
                    over: "items_json".into(),
                    verify_each: false,
                    verify_step: None,
                });
                s.max_retries = 0;
                s
            },
        ],
    };

    let mut engine = WorkflowEngine::new(def, "task".into());
    engine.mark_running();
    engine.record_outcome(StepOutcome::Completed {
        output: "ITEMS_JSON: [\"a\"]".into(),
    });

    engine.init_loop().unwrap();

    // Permanent failure in iteration
    engine.mark_running();
    let has_more = engine.record_outcome(StepOutcome::Failed {
        reason: "critical error".into(),
    });
    assert!(!has_more);
    assert!(engine.state().is_failed());
}

// ============= Verify each tests =============

#[test]
fn engine_verify_each_pass_advances() {
    let def = WorkflowDef {
        name: "test".into(),
        description: String::new(),
        agents: vec![AgentDef {
            id: "bot".into(),
            name: "Bot".into(),
            system_prompt: "".into(),
            profile: None,
        }],
        steps: vec![
            step("plan", "Plan", "{{input}}", vec![]),
            {
                let mut s = step("fix", "Fix", "Do {{current_item}}", vec!["plan"]);
                s.loop_config = Some(LoopConfig {
                    over: "items_json".into(),
                    verify_each: true,
                    verify_step: Some("verify".into()),
                });
                s
            },
            step(
                "verify",
                "Verify",
                "Check {{current_item}}: {{iteration_output}}",
                vec![],
            ),
        ],
    };

    let mut engine = WorkflowEngine::new(def, "task".into());
    engine.mark_running();
    engine.record_outcome(StepOutcome::Completed {
        output: "ITEMS_JSON: [\"story-1\", \"story-2\"]".into(),
    });

    engine.init_loop().unwrap();

    // Iteration 1: execute
    engine.mark_running();
    engine.record_outcome(StepOutcome::Completed {
        output: "Implemented story 1".into(),
    });

    // Should now need verification
    assert!(engine.needs_verify());

    // Get verify context
    let verify_ctx = engine.current_verify_context().unwrap().unwrap();
    assert!(verify_ctx.is_verify);
    assert!(verify_ctx.user_prompt.contains("story-1"));
    assert!(verify_ctx.user_prompt.contains("Implemented story 1"));

    // Verify passes
    engine.record_verify_outcome(StepOutcome::Completed {
        output: "STATUS: pass\nLooks good!".into(),
    });

    assert!(!engine.needs_verify());

    // Iteration 2
    let ctx = engine.current_step_context().unwrap().unwrap();
    assert_eq!(ctx.loop_iteration, Some((1, 2)));

    engine.mark_running();
    engine.record_outcome(StepOutcome::Completed {
        output: "Implemented story 2".into(),
    });

    // Verify iteration 2
    assert!(engine.needs_verify());
    engine.record_verify_outcome(StepOutcome::Completed {
        output: "STATUS: pass".into(),
    });

    // Loop is done, but verify step is still pending as a top-level step.
    // In the runner, is_current_verify_only() would auto-skip it.
    assert!(engine.is_current_verify_only());
    engine.skip_current();

    assert!(engine.state().is_complete());
}

#[test]
fn engine_verify_each_retry_repeats_iteration() {
    let def = WorkflowDef {
        name: "test".into(),
        description: String::new(),
        agents: vec![AgentDef {
            id: "bot".into(),
            name: "Bot".into(),
            system_prompt: "".into(),
            profile: None,
        }],
        steps: vec![
            step("plan", "Plan", "{{input}}", vec![]),
            {
                let mut s = step("fix", "Fix", "Do {{current_item}}", vec!["plan"]);
                s.loop_config = Some(LoopConfig {
                    over: "items_json".into(),
                    verify_each: true,
                    verify_step: Some("verify".into()),
                });
                s.max_retries = 3;
                s
            },
            step("verify", "Verify", "Check {{iteration_output}}", vec![]),
        ],
    };

    let mut engine = WorkflowEngine::new(def, "task".into());
    engine.mark_running();
    engine.record_outcome(StepOutcome::Completed {
        output: "ITEMS_JSON: [\"story-1\"]".into(),
    });

    engine.init_loop().unwrap();

    // First attempt
    engine.mark_running();
    engine.record_outcome(StepOutcome::Completed {
        output: "Bad implementation".into(),
    });

    // Verify says retry
    assert!(engine.needs_verify());
    engine.record_verify_outcome(StepOutcome::Completed {
        output: "STATUS: retry\nNeeds fixes".into(),
    });

    // Should be back to pending, same iteration
    assert_eq!(engine.state().steps[1].status, StepStatus::Pending);
    let ls = engine.state().steps[1].loop_state.as_ref().unwrap();
    assert_eq!(ls.iteration_retries, 1); // Per-iteration retry counter
    assert_eq!(ls.current_index, 0); // Still on first item
    assert!(ls.iteration_outputs[0].is_none()); // Output cleared

    // Second attempt succeeds
    engine.mark_running();
    engine.record_outcome(StepOutcome::Completed {
        output: "Good implementation".into(),
    });

    // Verify passes
    engine.record_verify_outcome(StepOutcome::Completed {
        output: "STATUS: pass".into(),
    });

    // Skip the verify step at top level
    assert!(engine.is_current_verify_only());
    engine.skip_current();

    assert!(engine.state().is_complete());
}

// ============= Conditional `when` tests =============

#[test]
fn engine_when_condition_equality() {
    let def = WorkflowDef {
        name: "test".into(),
        description: String::new(),
        agents: vec![AgentDef {
            id: "bot".into(),
            name: "Bot".into(),
            system_prompt: "".into(),
            profile: None,
        }],
        steps: vec![
            step("s1", "S1", "{{input}}", vec![]),
            {
                let mut s = step("s2", "S2", "remediate", vec![]);
                s.when = Some("{{verdict}} == FAIL".into());
                s
            },
            step("s3", "S3", "final", vec![]),
        ],
    };

    // Case 1: VERDICT = PASS → skip s2
    let mut engine = WorkflowEngine::new(def.clone(), "task".into());
    engine.mark_running();
    engine.record_outcome(StepOutcome::Completed {
        output: "VERDICT: PASS".into(),
    });

    assert!(!engine.evaluate_condition()); // s2 condition is false
    engine.skip_current();
    assert_eq!(engine.state().steps[1].status, StepStatus::Skipped);
    assert_eq!(engine.state().current_step, 2);

    // Case 2: VERDICT = FAIL → run s2
    let mut engine2 = WorkflowEngine::new(def, "task".into());
    engine2.mark_running();
    engine2.record_outcome(StepOutcome::Completed {
        output: "VERDICT: FAIL".into(),
    });

    assert!(engine2.evaluate_condition()); // s2 condition is true
}

#[test]
fn engine_when_condition_inequality() {
    let def = WorkflowDef {
        name: "test".into(),
        description: String::new(),
        agents: vec![AgentDef {
            id: "bot".into(),
            name: "Bot".into(),
            system_prompt: "".into(),
            profile: None,
        }],
        steps: vec![
            step("s1", "S1", "{{input}}", vec![]),
            {
                let mut s = step("s2", "S2", "do work", vec![]);
                s.when = Some("{{status}} != skip".into());
                s
            },
        ],
    };

    let mut engine = WorkflowEngine::new(def, "task".into());
    engine.mark_running();
    engine.record_outcome(StepOutcome::Completed {
        output: "STATUS: skip".into(),
    });

    assert!(!engine.evaluate_condition()); // "skip" != "skip" is false
}

#[test]
fn engine_when_no_condition_always_runs() {
    let engine = WorkflowEngine::new(simple_def(), "task".into());
    assert!(engine.evaluate_condition()); // No when = always true
}

// ============= Timeout config tests =============

#[test]
fn engine_timeout_in_step_context() {
    let def = WorkflowDef {
        name: "test".into(),
        description: String::new(),
        agents: vec![AgentDef {
            id: "bot".into(),
            name: "Bot".into(),
            system_prompt: "".into(),
            profile: None,
        }],
        steps: vec![{
            let mut s = step("s1", "S1", "{{input}}", vec![]);
            s.timeout_seconds = Some(30);
            s
        }],
    };

    let engine = WorkflowEngine::new(def, "task".into());
    let ctx = engine.current_step_context().unwrap().unwrap();
    assert_eq!(ctx.timeout_seconds, Some(30));
}

// ============= Persistence tests =============

#[test]
fn state_serialization_roundtrip() {
    let mut state =
        WorkflowState::new("test".into(), "input".into(), vec!["s1".into(), "s2".into()]);
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
    let db = std::sync::Arc::new(
        opengoose_persistence::Database::open_in_memory().unwrap(),
    );
    let store = WorkflowStore::new(db);

    let state = WorkflowState::new("myflow".into(), "hello".into(), vec!["s1".into()]);
    store.save("run-001", None, &state).unwrap();

    let loaded = store.load("run-001", "myflow").unwrap();
    assert_eq!(loaded.workflow_name, "myflow");
    assert_eq!(loaded.input, "hello");

    let runs = store.list_runs("myflow").unwrap();
    assert!(runs.contains(&"run-001".into()));

    store.remove("run-001", "myflow").unwrap();
    assert!(store.load("run-001", "myflow").is_err());
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
            profile: None,
        }],
        steps: vec![
            step("s1", "S1", "{{input}}", vec![]),
            step("s2", "S2", "continue", vec![]),
        ],
    };

    let mut state =
        WorkflowState::new("test".into(), "task".into(), vec!["s1".into(), "s2".into()]);
    state.steps[0].status = StepStatus::Completed;
    state.steps[0].output = Some("s1 output".into());
    state.current_step = 1;

    let mut engine = WorkflowEngine::resume(def, state).unwrap();

    let ctx = engine.current_step_context().unwrap().unwrap();
    assert_eq!(ctx.step_id, "s2");

    engine.mark_running();
    let has_more = engine.record_outcome(StepOutcome::Completed {
        output: "done".into(),
    });
    assert!(!has_more);
    assert!(engine.state().is_complete());
}

// ============= Last completed output tests =============

#[test]
fn state_last_completed_output_skips_skipped() {
    let mut state = WorkflowState::new(
        "test".into(),
        "input".into(),
        vec!["s1".into(), "s2".into(), "s3".into()],
    );
    state.steps[0].status = StepStatus::Completed;
    state.steps[0].output = Some("output-1".into());
    state.steps[1].status = StepStatus::Completed;
    state.steps[1].output = Some("output-2".into());
    state.steps[2].status = StepStatus::Skipped;

    // Last step is skipped, should get output-2
    assert_eq!(state.last_completed_output(), Some("output-2"));
}

#[test]
fn state_last_completed_output_returns_last_completed() {
    let mut state = WorkflowState::new(
        "test".into(),
        "input".into(),
        vec!["s1".into(), "s2".into()],
    );
    state.steps[0].status = StepStatus::Completed;
    state.steps[0].output = Some("output-1".into());
    state.steps[1].status = StepStatus::Completed;
    state.steps[1].output = Some("output-2".into());

    assert_eq!(state.last_completed_output(), Some("output-2"));
}

// ============= Empty loop test =============

#[test]
fn engine_loop_empty_items_skips_with_output() {
    let def = WorkflowDef {
        name: "test".into(),
        description: String::new(),
        agents: vec![AgentDef {
            id: "bot".into(),
            name: "Bot".into(),
            system_prompt: "".into(),
            profile: None,
        }],
        steps: vec![
            step("plan", "Plan", "{{input}}", vec![]),
            {
                let mut s = step("fix", "Fix", "Do {{current_item}}", vec!["plan"]);
                s.loop_config = Some(LoopConfig {
                    over: "items_json".into(),
                    verify_each: false,
                    verify_step: None,
                });
                s
            },
        ],
    };

    let mut engine = WorkflowEngine::new(def, "task".into());
    engine.mark_running();
    engine.record_outcome(StepOutcome::Completed {
        output: "ITEMS_JSON: []".into(),
    });

    let inited = engine.init_loop().unwrap();
    assert!(!inited); // Empty loop → skipped
    assert_eq!(engine.state().steps[1].status, StepStatus::Skipped);
    assert!(engine.state().steps[1].output.is_some()); // Has output message
}
