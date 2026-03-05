use opengoose_workflows::*;

const VALID_YAML: &str = r#"
name: test-workflow
description: A test workflow
agents:
  - id: bot
    name: Bot
    system_prompt: You are a bot
steps:
  - id: step1
    name: First step
    agent: bot
    prompt: "Do {{input}}"
  - id: step2
    name: Second step
    agent: bot
    prompt: "Continue from {{step1}}"
    depends_on: [step1]
    expects:
      - Output is valid
"#;

#[test]
fn loader_parses_valid_yaml() {
    let mut loader = WorkflowLoader::new();
    loader.load_str(VALID_YAML).unwrap();

    let names = loader.list();
    assert!(names.contains(&"test-workflow"));

    let def = loader.get("test-workflow").unwrap();
    assert_eq!(def.agents.len(), 1);
    assert_eq!(def.steps.len(), 2);
    assert_eq!(def.steps[1].depends_on, vec!["step1"]);
    assert_eq!(def.steps[1].expects, vec!["Output is valid"]);
}

#[test]
fn loader_rejects_empty_name() {
    let yaml = r#"
name: ""
agents:
  - id: bot
    name: Bot
    system_prompt: test
steps:
  - id: s
    name: S
    agent: bot
    prompt: test
"#;
    let mut loader = WorkflowLoader::new();
    let err = loader.load_str(yaml).unwrap_err();
    assert!(err.to_string().contains("name cannot be empty"));
}

#[test]
fn loader_rejects_no_steps() {
    let yaml = r#"
name: empty
agents:
  - id: bot
    name: Bot
    system_prompt: test
steps: []
"#;
    let mut loader = WorkflowLoader::new();
    let err = loader.load_str(yaml).unwrap_err();
    assert!(err.to_string().contains("at least one step"));
}

#[test]
fn loader_rejects_unknown_agent() {
    let yaml = r#"
name: bad-agent
agents:
  - id: bot
    name: Bot
    system_prompt: test
steps:
  - id: s
    name: S
    agent: ghost
    prompt: test
"#;
    let mut loader = WorkflowLoader::new();
    let err = loader.load_str(yaml).unwrap_err();
    assert!(err.to_string().contains("unknown agent 'ghost'"));
}

#[test]
fn loader_rejects_unknown_dependency() {
    let yaml = r#"
name: bad-dep
agents:
  - id: bot
    name: Bot
    system_prompt: test
steps:
  - id: s
    name: S
    agent: bot
    prompt: test
    depends_on: [nonexistent]
"#;
    let mut loader = WorkflowLoader::new();
    let err = loader.load_str(yaml).unwrap_err();
    assert!(err.to_string().contains("unknown step 'nonexistent'"));
}

#[test]
fn loader_rejects_self_dependency() {
    let yaml = r#"
name: self-dep
agents:
  - id: bot
    name: Bot
    system_prompt: test
steps:
  - id: loop
    name: Loop
    agent: bot
    prompt: test
    depends_on: [loop]
"#;
    let mut loader = WorkflowLoader::new();
    let err = loader.load_str(yaml).unwrap_err();
    assert!(err.to_string().contains("circular") || err.to_string().contains("yclic"));
}

#[test]
fn loader_rejects_transitive_cycle() {
    let yaml = r#"
name: cycle
agents:
  - id: bot
    name: Bot
    system_prompt: test
steps:
  - id: a
    name: A
    agent: bot
    prompt: test
    depends_on: [c]
  - id: b
    name: B
    agent: bot
    prompt: test
    depends_on: [a]
  - id: c
    name: C
    agent: bot
    prompt: test
    depends_on: [b]
"#;
    let mut loader = WorkflowLoader::new();
    let err = loader.load_str(yaml).unwrap_err();
    let msg = err.to_string();
    assert!(
        msg.contains("circular") || msg.contains("yclic") || msg.contains("appears at or after"),
        "expected cycle or ordering error, got: {msg}"
    );
}

#[test]
fn loader_rejects_duplicate_step_ids() {
    let yaml = r#"
name: dupes
agents:
  - id: bot
    name: Bot
    system_prompt: test
steps:
  - id: same
    name: First
    agent: bot
    prompt: test
  - id: same
    name: Second
    agent: bot
    prompt: test
"#;
    let mut loader = WorkflowLoader::new();
    let err = loader.load_str(yaml).unwrap_err();
    assert!(err.to_string().contains("duplicate"));
}

#[test]
fn loader_defaults_max_retries_and_on_fail() {
    let mut loader = WorkflowLoader::new();
    loader.load_str(VALID_YAML).unwrap();
    let def = loader.get("test-workflow").unwrap();

    assert_eq!(def.steps[0].max_retries, 2);
    assert_eq!(def.steps[0].on_fail, OnFailStrategy::Abort);
}

#[test]
fn loader_parses_on_fail_skip() {
    let yaml = r#"
name: skippable
agents:
  - id: bot
    name: Bot
    system_prompt: test
steps:
  - id: s
    name: S
    agent: bot
    prompt: test
    on_fail: skip
"#;
    let mut loader = WorkflowLoader::new();
    loader.load_str(yaml).unwrap();
    let def = loader.get("skippable").unwrap();
    assert_eq!(def.steps[0].on_fail, OnFailStrategy::Skip);
}

#[test]
fn loader_loads_bundled_workflows() {
    let mut loader = WorkflowLoader::new();
    let count = loader.load_dir(&WorkflowLoader::bundled_dir()).unwrap();
    assert!(count >= 2, "expected at least 2 bundled workflows, got {count}");
    assert!(loader.get("feature-dev").is_some());
    assert!(loader.get("bug-fix").is_some());
}

#[test]
fn loader_validates_loop_over_not_empty() {
    let yaml = r#"
name: bad-loop
agents:
  - id: bot
    name: Bot
    system_prompt: test
steps:
  - id: s
    name: S
    agent: bot
    prompt: test
    loop:
      over: ""
"#;
    let mut loader = WorkflowLoader::new();
    let err = loader.load_str(yaml).unwrap_err();
    assert!(err.to_string().contains("loop.over cannot be empty"), "got: {err}");
}

#[test]
fn loader_validates_verify_each_requires_verify_step() {
    let yaml = r#"
name: bad-verify
agents:
  - id: bot
    name: Bot
    system_prompt: test
steps:
  - id: s
    name: S
    agent: bot
    prompt: test
    loop:
      over: items
      verify_each: true
"#;
    let mut loader = WorkflowLoader::new();
    let err = loader.load_str(yaml).unwrap_err();
    assert!(err.to_string().contains("verify_each is true but no verify_step"), "got: {err}");
}

#[test]
fn loader_validates_verify_step_exists() {
    let yaml = r#"
name: bad-verify-ref
agents:
  - id: bot
    name: Bot
    system_prompt: test
steps:
  - id: s
    name: S
    agent: bot
    prompt: test
    loop:
      over: items
      verify_each: true
      verify_step: nonexistent
"#;
    let mut loader = WorkflowLoader::new();
    let err = loader.load_str(yaml).unwrap_err();
    assert!(err.to_string().contains("references unknown step"), "got: {err}");
}

#[test]
fn loader_parses_timeout_seconds() {
    let yaml = r#"
name: timeout-test
agents:
  - id: bot
    name: Bot
    system_prompt: test
steps:
  - id: s
    name: S
    agent: bot
    prompt: test
    timeout_seconds: 60
"#;
    let mut loader = WorkflowLoader::new();
    loader.load_str(yaml).unwrap();
    let def = loader.get("timeout-test").unwrap();
    assert_eq!(def.steps[0].timeout_seconds, Some(60));
}

#[test]
fn loader_parses_when_condition() {
    let yaml = r#"
name: when-test
agents:
  - id: bot
    name: Bot
    system_prompt: test
steps:
  - id: s
    name: S
    agent: bot
    prompt: test
    when: "{{status}} == pass"
"#;
    let mut loader = WorkflowLoader::new();
    loader.load_str(yaml).unwrap();
    let def = loader.get("when-test").unwrap();
    assert_eq!(def.steps[0].when.as_deref(), Some("{{status}} == pass"));
}

#[test]
fn loader_accepts_agent_with_profile() {
    let yaml = r#"
name: profile-test
agents:
  - id: researcher
    name: Researcher
    profile: senior-researcher
steps:
  - id: s
    name: S
    agent: researcher
    prompt: test
"#;
    let mut loader = WorkflowLoader::new();
    loader.load_str(yaml).unwrap();
    let def = loader.get("profile-test").unwrap();
    assert_eq!(def.agents[0].profile.as_deref(), Some("senior-researcher"));
    assert!(def.agents[0].system_prompt.is_empty());
}

#[test]
fn loader_rejects_agent_without_prompt_or_profile() {
    let yaml = r#"
name: no-prompt
agents:
  - id: bot
    name: Bot
steps:
  - id: s
    name: S
    agent: bot
    prompt: test
"#;
    let mut loader = WorkflowLoader::new();
    let err = loader.load_str(yaml).unwrap_err();
    assert!(
        err.to_string().contains("must have either system_prompt or profile"),
        "got: {err}"
    );
}
