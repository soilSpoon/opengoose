use super::types::AgentOutput;

/// Parse an agent's raw response text for @mentions and [BROADCAST] tags.
///
/// - `@agent_name: message` → delegation to another agent
/// - `[BROADCAST]: message` → broadcast to shared context log
///
/// Returns cleaned response text with these lines stripped.
pub fn parse_agent_output(raw: &str) -> AgentOutput {
    let mut response_lines = Vec::new();
    let mut delegations = Vec::new();
    let mut broadcasts = Vec::new();

    for line in raw.lines() {
        let trimmed = line.trim();

        // Detect [BROADCAST]: ...
        if let Some(rest) = trimmed.strip_prefix("[BROADCAST]:") {
            broadcasts.push(rest.trim().to_string());
            continue;
        }

        // Detect @agent_name: message or @agent_name message
        if trimmed.starts_with('@')
            && let Some((agent, msg)) = parse_mention(trimmed)
        {
            delegations.push((agent, msg));
            continue;
        }

        response_lines.push(line);
    }

    let response = response_lines.join("\n").trim().to_string();

    AgentOutput {
        response,
        delegations,
        broadcasts,
    }
}

/// Parse an @mention line. Returns (agent_name, message) or None.
fn parse_mention(line: &str) -> Option<(String, String)> {
    let without_at = line.strip_prefix('@')?;

    // Try `@agent_name: message` first
    if let Some((agent, msg)) = without_at.split_once(':') {
        let agent = agent.trim();
        let msg = msg.trim();
        if !agent.is_empty() && !agent.contains(' ') && !msg.is_empty() {
            return Some((agent.to_string(), msg.to_string()));
        }
    }

    // Try `@agent_name message` (first word is agent, rest is message)
    let mut parts = without_at.splitn(2, ' ');
    let agent = parts.next()?.trim();
    let msg = parts.next()?.trim();
    if !agent.is_empty() && !msg.is_empty() {
        Some((agent.to_string(), msg.to_string()))
    } else {
        None
    }
}
