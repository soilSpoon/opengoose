// Board Platform Extension — 에이전트가 보드에 접근하는 내장 도구
//
// McpClientTrait 직접 구현. MCP JSON-RPC 직렬화 오버헤드 제로.
// ExtensionManager::add_client()로 Agent에 주입.

use async_trait::async_trait;
use goose::agents::mcp_client::{Error, McpClientTrait};
use opengoose_board::work_item::{PostWorkItem, Priority, RigId};
use opengoose_board::Board;
use rmcp::model::{
    CallToolResult, Content, Implementation, InitializeResult, JsonObject, ListToolsResult,
    ProtocolVersion, ServerCapabilities, Tool,
};
use serde_json::{Value, json};
use std::borrow::Cow;
use std::sync::Arc;
use tokio::sync::Mutex;
use tokio_util::sync::CancellationToken;

pub struct BoardClient {
    info: InitializeResult,
    board: Arc<Mutex<Board>>,
    rig_id: RigId,
}

impl BoardClient {
    pub fn new(board: Arc<Mutex<Board>>, rig_id: RigId) -> Self {
        Self {
            info: InitializeResult {
                protocol_version: ProtocolVersion::V_2025_03_26,
                capabilities: ServerCapabilities {
                    tools: None,
                    tasks: None,
                    resources: None,
                    prompts: None,
                    completions: None,
                    experimental: None,
                    logging: None,
                    extensions: None,
                },
                server_info: Implementation {
                    name: "board".to_string(),
                    title: Some("Wanted Board".to_string()),
                    version: "0.2.0".to_string(),
                    description: None,
                    icons: None,
                    website_url: None,
                },
                instructions: None,
            },
            board,
            rig_id,
        }
    }

    fn tools() -> Vec<Tool> {
        vec![
            tool_def(
                "read_board",
                "Show current board status: open, claimed, and recent done items.",
                json!({"type": "object", "properties": {}}),
            ),
            tool_def(
                "claim_next",
                "Claim the highest-priority open work item from the board.",
                json!({"type": "object", "properties": {}}),
            ),
            tool_def(
                "submit",
                "Mark the current work item as done.",
                json!({
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
                json!({
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

    async fn handle_read_board(&self) -> CallToolResult {
        let board = self.board.lock().await;
        let items = board.list();

        let open: Vec<_> = items.iter().filter(|i| i.status == opengoose_board::Status::Open).collect();
        let claimed: Vec<_> = items.iter().filter(|i| i.status == opengoose_board::Status::Claimed).collect();
        let done: Vec<_> = items.iter().filter(|i| i.status == opengoose_board::Status::Done).collect();

        let mut text = format!("Board: {} open, {} claimed, {} done\n", open.len(), claimed.len(), done.len());

        if !open.is_empty() {
            text.push_str("\nOpen:\n");
            for item in &open {
                text.push_str(&format!("  #{} {:?} \"{}\"\n", item.id, item.priority, item.title));
            }
        }

        if !claimed.is_empty() {
            text.push_str("\nClaimed:\n");
            for item in &claimed {
                let by = item.claimed_by.as_ref().map(|r| r.0.as_str()).unwrap_or("?");
                text.push_str(&format!("  #{} \"{}\" (by {})\n", item.id, item.title, by));
            }
        }

        if !done.is_empty() {
            text.push_str("\nRecent done:\n");
            for item in done.iter().rev().take(5) {
                text.push_str(&format!("  #{} \"{}\"\n", item.id, item.title));
            }
        }

        CallToolResult::success(vec![Content::text(text)])
    }

    async fn handle_claim_next(&self) -> CallToolResult {
        let mut board = self.board.lock().await;
        let ready = board.ready();

        let Some(item) = ready.first() else {
            return CallToolResult::success(vec![Content::text("No open items available.")]);
        };

        let item_id = item.id;
        match board.claim(item_id, &self.rig_id) {
            Ok(claimed) => CallToolResult::success(vec![Content::text(format!(
                "Claimed #{}: \"{}\" ({:?})",
                claimed.id, claimed.title, claimed.priority
            ))]),
            Err(e) => CallToolResult::error(vec![Content::text(format!("Claim failed: {e}"))]),
        }
    }

    async fn handle_submit(&self, args: &JsonObject) -> CallToolResult {
        let Some(item_id) = args.get("item_id").and_then(Value::as_i64) else {
            return CallToolResult::error(vec![Content::text("Missing item_id")]);
        };

        let mut board = self.board.lock().await;
        match board.submit(item_id, &self.rig_id) {
            Ok(done) => CallToolResult::success(vec![Content::text(format!(
                "Completed #{}: \"{}\"",
                done.id, done.title
            ))]),
            Err(e) => CallToolResult::error(vec![Content::text(format!("Submit failed: {e}"))]),
        }
    }

    async fn handle_create_task(&self, args: &JsonObject) -> CallToolResult {
        let Some(title) = args.get("title").and_then(Value::as_str) else {
            return CallToolResult::error(vec![Content::text("Missing title")]);
        };

        let description = args
            .get("description")
            .and_then(Value::as_str)
            .unwrap_or("")
            .to_string();

        let priority = match args.get("priority").and_then(Value::as_str) {
            Some("P0") => Priority::P0,
            Some("P2") => Priority::P2,
            _ => Priority::P1,
        };

        let mut board = self.board.lock().await;
        let item = board.post(PostWorkItem {
            title: title.to_string(),
            description,
            created_by: self.rig_id.clone(),
            priority,
        });

        CallToolResult::success(vec![Content::text(format!(
            "Created #{}: \"{}\" ({:?})",
            item.id, item.title, item.priority
        ))])
    }
}

#[async_trait]
impl McpClientTrait for BoardClient {
    async fn list_tools(
        &self,
        _session_id: &str,
        _next_cursor: Option<String>,
        _cancellation_token: CancellationToken,
    ) -> Result<ListToolsResult, Error> {
        Ok(ListToolsResult {
            tools: Self::tools(),
            next_cursor: None,
            meta: None,
        })
    }

    async fn call_tool(
        &self,
        _session_id: &str,
        name: &str,
        arguments: Option<JsonObject>,
        _working_dir: Option<&str>,
        _cancellation_token: CancellationToken,
    ) -> Result<CallToolResult, Error> {
        let args = arguments.unwrap_or_default();
        let result = match name {
            "read_board" => self.handle_read_board().await,
            "claim_next" => self.handle_claim_next().await,
            "submit" => self.handle_submit(&args).await,
            "create_task" => self.handle_create_task(&args).await,
            other => CallToolResult::error(vec![Content::text(format!("Unknown tool: {other}"))]),
        };
        Ok(result)
    }

    fn get_info(&self) -> Option<&InitializeResult> {
        Some(&self.info)
    }
}

// ── Helper ───────────────────────────────────────────────────

fn tool_def(name: &str, description: &str, schema: Value) -> Tool {
    let schema_obj: JsonObject = serde_json::from_value(schema).unwrap_or_default();
    Tool {
        name: Cow::Owned(name.to_string()),
        title: None,
        description: Some(Cow::Owned(description.to_string())),
        input_schema: Arc::new(schema_obj),
        output_schema: None,
        annotations: None,
        execution: None,
        icons: None,
        meta: None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use opengoose_board::Board;

    fn make_board_client() -> (BoardClient, Arc<Mutex<Board>>) {
        let board = Arc::new(Mutex::new(Board::new()));
        let client = BoardClient::new(Arc::clone(&board), RigId::new("test-rig"));
        (client, board)
    }

    #[tokio::test]
    async fn list_tools_returns_four() {
        let (client, _) = make_board_client();
        let cancel = CancellationToken::new();
        let result = client.list_tools("s1", None, cancel).await.unwrap();
        assert_eq!(result.tools.len(), 4);
    }

    #[tokio::test]
    async fn read_board_empty() {
        let (client, _) = make_board_client();
        let result = client.handle_read_board().await;
        let text = content_text(&result);
        assert!(text.contains("0 open"));
    }

    #[tokio::test]
    async fn create_and_claim_and_submit() {
        let (client, _) = make_board_client();

        // create
        let mut args = JsonObject::new();
        args.insert("title".into(), json!("test task"));
        let result = client.handle_create_task(&args).await;
        let text = content_text(&result);
        assert!(text.contains("Created #1"));

        // claim
        let result = client.handle_claim_next().await;
        let text = content_text(&result);
        assert!(text.contains("Claimed #1"));

        // submit
        let mut args = JsonObject::new();
        args.insert("item_id".into(), json!(1));
        let result = client.handle_submit(&args).await;
        let text = content_text(&result);
        assert!(text.contains("Completed #1"));
    }

    #[tokio::test]
    async fn claim_empty_board() {
        let (client, _) = make_board_client();
        let result = client.handle_claim_next().await;
        let text = content_text(&result);
        assert!(text.contains("No open items"));
    }

    fn content_text(result: &CallToolResult) -> String {
        result
            .content
            .iter()
            .filter_map(|c| match &c.raw {
                rmcp::model::RawContent::Text(t) => Some(t.text.clone()),
                _ => None,
            })
            .collect::<Vec<_>>()
            .join("")
    }
}
