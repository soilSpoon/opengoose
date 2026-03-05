use tracing::warn;

use crate::context::OrchestrationContext;

// ── Free functions ──────────────────────────────────────────────────

/// Load conversation history as `(role, content)` pairs for seeding into
/// a Goose session via `AgentRunner::seed_history()`.
///
/// This is the preferred way to pass history — it uses Goose's native
/// session management rather than baking text into the prompt.
pub fn load_history_pairs(ctx: &OrchestrationContext) -> Vec<(String, String)> {
    match ctx.sessions().load_history(&ctx.session_key, 20) {
        Ok(history) => history
            .into_iter()
            .map(|h| (h.role, h.content))
            .collect(),
        Err(e) => {
            warn!("failed to load conversation history: {e}");
            Vec::new()
        }
    }
}

/// Build the role context suffix from an optional role description.
pub fn build_role_context(role: Option<&str>, label: &str) -> String {
    role.map(|r| format!("\n\n[{label}: {r}]"))
        .unwrap_or_default()
}

/// Format broadcasts into a context section for agent input.
pub fn format_broadcast_context(ctx: &OrchestrationContext, header: &str) -> String {
    let broadcasts = ctx.read_broadcasts(None);
    if broadcasts.is_empty() {
        String::new()
    } else {
        let text: String = broadcasts
            .iter()
            .map(|b| format!("- [{}]: {}", b.sender, b.content))
            .collect::<Vec<_>>()
            .join("\n");
        format!("\n\n[{header}]:\n{text}")
    }
}
