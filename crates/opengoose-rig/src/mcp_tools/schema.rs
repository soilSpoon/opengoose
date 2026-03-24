// Tool schema definitions — MCP tool metadata for board operations

use rmcp::model::{JsonObject, Tool};
use serde_json::Value;
use std::borrow::Cow;
use std::sync::Arc;

pub fn board_tools() -> Vec<Tool> {
    vec![
        tool_def(
            "read_board",
            "Show current board status: open, claimed, and recent done items.",
            serde_json::json!({"type": "object", "properties": {}}),
        ),
        tool_def(
            "claim_next",
            "Claim the highest-priority open work item from the board.",
            serde_json::json!({"type": "object", "properties": {}}),
        ),
        tool_def(
            "submit",
            "Mark the current work item as done.",
            serde_json::json!({
                "type": "object",
                "properties": {
                    "item_id": {"type": "integer", "description": "Work item ID to complete"}
                },
                "required": ["item_id"]
            }),
        ),
        tool_def(
            "create_task",
            "Post a new work item to the board.",
            serde_json::json!({
                "type": "object",
                "properties": {
                    "title": {"type": "string", "description": "Task title"},
                    "description": {"type": "string", "description": "Task description"},
                    "priority": {"type": "string", "enum": ["P0", "P1", "P2"], "description": "Priority level"}
                },
                "required": ["title"]
            }),
        ),
    ]
}

fn tool_def(name: &str, description: &str, schema: Value) -> Tool {
    let schema_obj: JsonObject = serde_json::from_value(schema).unwrap_or_default();
    let mut tool = Tool::default();
    tool.name = Cow::Owned(name.to_string());
    tool.description = Some(Cow::Owned(description.to_string()));
    tool.input_schema = Arc::new(schema_obj);
    tool
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn board_tools_returns_four() {
        let tools = board_tools();
        assert_eq!(tools.len(), 4);
    }
}
