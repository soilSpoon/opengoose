use super::output::parse_agent_output;
use super::types::{AgentEventSummary, AgentOutput};

#[test]
fn test_parse_broadcast() {
    let output = parse_agent_output(
        "Here's my analysis.\n[BROADCAST]: Found critical auth bug in line 42\nMore details here.",
    );
    assert_eq!(output.broadcasts.len(), 1);
    assert_eq!(output.broadcasts[0], "Found critical auth bug in line 42");
    assert_eq!(output.response, "Here's my analysis.\nMore details here.");
}

#[test]
fn test_parse_mention_colon() {
    let output = parse_agent_output("@reviewer: please check the auth module");
    assert_eq!(output.delegations.len(), 1);
    assert_eq!(output.delegations[0].0, "reviewer");
    assert_eq!(output.delegations[0].1, "please check the auth module");
    assert!(output.response.is_empty());
}

#[test]
fn test_parse_mention_space() {
    let output = parse_agent_output("@coder fix the bug in auth.rs");
    assert_eq!(output.delegations.len(), 1);
    assert_eq!(output.delegations[0].0, "coder");
    assert_eq!(output.delegations[0].1, "fix the bug in auth.rs");
}

#[test]
fn test_mixed_output() {
    let raw = "Starting analysis.\n\
               [BROADCAST]: database schema looks outdated\n\
               @coder: update the migration files\n\
               Here's the summary.\n\
               [BROADCAST]: tests are all passing";
    let output = parse_agent_output(raw);
    assert_eq!(output.broadcasts.len(), 2);
    assert_eq!(output.delegations.len(), 1);
    assert_eq!(output.response, "Starting analysis.\nHere's the summary.");
}

#[test]
fn test_no_special_output() {
    let output = parse_agent_output("Just a normal response with no special tags.");
    assert!(output.broadcasts.is_empty());
    assert!(output.delegations.is_empty());
    assert_eq!(
        output.response,
        "Just a normal response with no special tags."
    );
}

#[test]
fn test_parse_mention_at_only() {
    // "@" alone should not be parsed as a mention
    let output = parse_agent_output("@");
    assert!(output.delegations.is_empty());
    assert_eq!(output.response, "@");
}

#[test]
fn test_parse_mention_at_with_spaces() {
    // "@agent name with spaces: msg" — agent name has spaces, should not match colon form
    let output = parse_agent_output("@agent name with spaces: some message");
    // Falls through to space-based parsing: agent="agent", msg="name with spaces: some message"
    assert_eq!(output.delegations.len(), 1);
    assert_eq!(output.delegations[0].0, "agent");
}

#[test]
fn test_parse_mention_no_message() {
    // "@coder" alone (no message) should not be a delegation
    let output = parse_agent_output("@coder");
    assert!(output.delegations.is_empty());
    assert_eq!(output.response, "@coder");
}

#[test]
fn test_parse_mention_colon_empty_message() {
    // "@coder: " (empty after colon) — should not be parsed as delegation
    let output = parse_agent_output("@coder:");
    // colon form: agent="coder", msg="" → msg is empty → falls through to space form
    // space form: no space → returns None
    assert!(output.delegations.is_empty());
}

#[test]
fn test_parse_broadcast_whitespace() {
    let output = parse_agent_output("[BROADCAST]:    extra spaces   ");
    assert_eq!(output.broadcasts.len(), 1);
    assert_eq!(output.broadcasts[0], "extra spaces");
}

#[test]
fn test_parse_empty_input() {
    let output = parse_agent_output("");
    assert!(output.broadcasts.is_empty());
    assert!(output.delegations.is_empty());
    assert_eq!(output.response, "");
}

#[test]
fn test_parse_only_whitespace_lines() {
    let output = parse_agent_output("  \n  \n  ");
    assert!(output.broadcasts.is_empty());
    assert!(output.delegations.is_empty());
}

#[test]
fn test_multiple_delegations() {
    let raw = "@coder: fix the bug\n@reviewer: check the fix\n@tester run the tests";
    let output = parse_agent_output(raw);
    assert_eq!(output.delegations.len(), 3);
    assert_eq!(
        output.delegations[0],
        ("coder".into(), "fix the bug".into())
    );
    assert_eq!(
        output.delegations[1],
        ("reviewer".into(), "check the fix".into())
    );
    assert_eq!(
        output.delegations[2],
        ("tester".into(), "run the tests".into())
    );
    assert!(output.response.is_empty());
}

#[test]
fn test_agent_event_summary_default() {
    let summary = AgentEventSummary::default();
    assert!(summary.model_changes.is_empty());
    assert_eq!(summary.context_compactions, 0);
    assert!(summary.extension_notifications.is_empty());
}

#[test]
fn test_agent_output_profile_name() {
    // Verify AgentOutput fields are Debug-printable
    let output = AgentOutput {
        response: "hello".into(),
        delegations: vec![("a".into(), "b".into())],
        broadcasts: vec!["msg".into()],
    };
    let debug = format!("{:?}", output);
    assert!(debug.contains("hello"));
    assert!(debug.contains("msg"));
}
