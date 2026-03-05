use tracing::warn;

use opengoose_persistence::HistoryMessage;

use crate::context::OrchestrationContext;

/// Encapsulates the repeated pattern of loading conversation history,
/// building role context, and formatting broadcast context into a
/// single reusable builder.
pub struct PromptContextBuilder {
    history_text: String,
    role_ctx: String,
    broadcast_ctx: String,
}

impl PromptContextBuilder {
    /// Build prompt context from the orchestration context.
    ///
    /// This replaces the duplicated sequence of `load_and_format_history`,
    /// `build_role_context`, and `format_broadcast_context` that was called
    /// 3-4 times across the different workflow paths.
    pub fn new(
        ctx: &OrchestrationContext,
        role: Option<&str>,
        role_label: &str,
        broadcast_header: &str,
    ) -> Self {
        let history_text = load_and_format_history(ctx);
        let role_ctx = build_role_context(role, role_label);
        let broadcast_ctx = format_broadcast_context(ctx, broadcast_header);

        Self {
            history_text,
            role_ctx,
            broadcast_ctx,
        }
    }

    /// Build context with only history (no role or broadcast).
    pub fn history_only(ctx: &OrchestrationContext) -> Self {
        let history_text = load_and_format_history(ctx);
        Self {
            history_text,
            role_ctx: String::new(),
            broadcast_ctx: String::new(),
        }
    }

    /// The formatted history text.
    pub fn history_text(&self) -> &str {
        &self.history_text
    }

    /// The role context suffix (empty if no role).
    pub fn role_ctx(&self) -> &str {
        &self.role_ctx
    }

    /// The broadcast context suffix (empty if no broadcasts).
    pub fn broadcast_ctx(&self) -> &str {
        &self.broadcast_ctx
    }

    /// Build the history prefix for prepending to agent input.
    pub fn history_prefix(&self) -> String {
        build_history_prefix(&self.history_text)
    }
}

// ── Free functions (extracted from TeamOrchestrator) ─────────────────

/// Load conversation history and format it as a text block.
pub fn load_and_format_history(ctx: &OrchestrationContext) -> String {
    let history = match ctx.sessions().load_history(&ctx.session_key, 20) {
        Ok(v) => v,
        Err(e) => {
            warn!("failed to load conversation history: {e}");
            Default::default()
        }
    };
    format_history(&history)
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

/// Build a history prefix string for prepending to agent input.
pub fn build_history_prefix(history_text: &str) -> String {
    if history_text.is_empty() {
        String::new()
    } else {
        format!("Conversation history:\n---\n{history_text}\n---\n\nCurrent message: ")
    }
}

/// Format conversation history into a text block for injection.
fn format_history(history: &[HistoryMessage]) -> String {
    if history.is_empty() {
        return String::new();
    }
    history
        .iter()
        .map(|h| format!("[{}]: {}", h.role, h.content))
        .collect::<Vec<_>>()
        .join("\n")
}
