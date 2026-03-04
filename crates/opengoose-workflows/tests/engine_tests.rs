use opengoose_workflows::*;

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
            StepDef {
                id: "step1".into(),
                name: "First".into(),
                agent: "bot".into(),
                prompt: "Do {{input}}".into(),
                expects: vec![],
                max_retries: 2,
                depends_on: vec![],
                on_fail: OnFailStrategy::Abort,
            },
            StepDef {
                id: "step2".into(),
                name: "Second".into(),
                agent: "bot".into(),
                prompt: "Continue from {{step1}}".into(),
                expects: vec!["Output is valid".into()],
                max_retries: 2,
                depends_on: vec!["step1".into()],
                on_fail: OnFailStrategy::Abort,
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

    // Complete step1 with output
    engine.mark_running();
    engine.record_outcome(StepOutcome::Completed {
        output: "step1 result".into(),
    });

    // step2 should have step1's output injected
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
            StepDef {
                id: "a".into(),
                name: "A".into(),
                agent: "bot".into(),
                prompt: "do A".into(),
                expects: vec![],
                max_retries: 0,
                depends_on: vec![],
                on_fail: OnFailStrategy::Abort,
            },
            StepDef {
                id: "b".into(),
                name: "B".into(),
                agent: "bot".into(),
                prompt: "use {{a}}".into(),
                expects: vec![],
                max_retries: 0,
                depends_on: vec!["a".into()],
                on_fail: OnFailStrategy::Abort,
            },
        ],
    };

    let mut engine = WorkflowEngine::new(def, "input".into());

    // Fail step A (so it has no output)
    engine.mark_running();
    engine.record_outcome(StepOutcome::Failed {
        reason: "broken".into(),
    });

    // Workflow should be terminal now (failed)
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
}

#[test]
fn engine_retry_then_succeed() {
    let mut engine = WorkflowEngine::new(simple_def(), "task".into());

    // First attempt fails (retry)
    engine.mark_running();
    let has_more = engine.record_outcome(StepOutcome::Retry {
        reason: "not good".into(),
    });
    assert!(has_more);
    assert_eq!(engine.state().steps[0].retries, 1);
    assert_eq!(engine.state().steps[0].status, StepStatus::Pending);

    // Second attempt succeeds
    engine.mark_running();
    let has_more = engine.record_outcome(StepOutcome::Completed {
        output: "ok".into(),
    });
    assert!(has_more); // still has step2
    assert_eq!(engine.state().steps[0].status, StepStatus::Completed);
    assert_eq!(engine.progress(), (1, 2));
}

#[test]
fn engine_retry_exhaustion_aborts() {
    let mut engine = WorkflowEngine::new(simple_def(), "task".into());

    // max_retries = 2, so 2 retries should exhaust
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
            StepDef {
                id: "optional".into(),
                name: "Optional".into(),
                agent: "bot".into(),
                prompt: "try this".into(),
                expects: vec![],
                max_retries: 0,
                depends_on: vec![],
                on_fail: OnFailStrategy::Skip,
            },
            StepDef {
                id: "required".into(),
                name: "Required".into(),
                agent: "bot".into(),
                prompt: "do this".into(),
                expects: vec![],
                max_retries: 2,
                depends_on: vec![],
                on_fail: OnFailStrategy::Abort,
            },
        ],
    };

    let mut engine = WorkflowEngine::new(def, "input".into());

    // Fail the optional step
    engine.mark_running();
    let has_more = engine.record_outcome(StepOutcome::Failed {
        reason: "not critical".into(),
    });

    // Should skip and continue to required step
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
        steps: vec![StepDef {
            id: "only".into(),
            name: "Only".into(),
            agent: "bot".into(),
            prompt: "{{input}}".into(),
            expects: vec![],
            max_retries: 2,
            depends_on: vec![],
            on_fail: OnFailStrategy::Abort,
        }],
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
