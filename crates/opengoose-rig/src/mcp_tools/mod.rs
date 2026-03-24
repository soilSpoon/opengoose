// Board Platform Extension — 에이전트가 보드에 접근하는 내장 도구
//
// McpClientTrait 직접 구현. MCP JSON-RPC 직렬화 오버헤드 제로.
// ExtensionManager::add_client()로 Agent에 주입.

mod handlers;
mod schema;

use async_trait::async_trait;
use goose::agents::ToolCallContext;
use goose::agents::mcp_client::{Error, McpClientTrait};
use opengoose_board::Board;
use opengoose_board::work_item::RigId;
use rmcp::model::{
    CallToolResult, Content, Implementation, InitializeResult, JsonObject, ListToolsResult,
    ProtocolVersion,
};
use std::sync::Arc;
use tokio_util::sync::CancellationToken;

pub struct BoardClient {
    info: InitializeResult,
    board: Arc<Board>,
    rig_id: RigId,
}

impl BoardClient {
    pub fn new(board: Arc<Board>, rig_id: RigId) -> Self {
        Self {
            info: {
                let mut info = InitializeResult::default();
                info.protocol_version = ProtocolVersion::V_2025_03_26;
                info.server_info = {
                    let mut imp = Implementation::default();
                    imp.name = "board".to_string();
                    imp.title = Some("Wanted Board".to_string());
                    imp.version = "0.2.0".to_string();
                    imp
                };
                info
            },
            board,
            rig_id,
        }
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
            tools: schema::board_tools(),
            next_cursor: None,
            meta: None,
        })
    }

    async fn call_tool(
        &self,
        _ctx: &ToolCallContext,
        name: &str,
        arguments: Option<JsonObject>,
        _cancellation_token: CancellationToken,
    ) -> Result<CallToolResult, Error> {
        let args = arguments.unwrap_or_default();
        let result = match name {
            "read_board" => handlers::handle_read_board(&self.board).await,
            "claim_next" => handlers::handle_claim_next(&self.board, &self.rig_id).await,
            "submit" => handlers::handle_submit(&self.board, &self.rig_id, &args).await,
            "create_task" => handlers::handle_create_task(&self.board, &self.rig_id, &args).await,
            other => CallToolResult::error(vec![Content::text(format!("Unknown tool: {other}"))]),
        };
        Ok(result)
    }

    fn get_info(&self) -> Option<&InitializeResult> {
        Some(&self.info)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use opengoose_board::Board;
    use serde_json::json;

    async fn make_board_client() -> BoardClient {
        let board = Arc::new(Board::in_memory().await.unwrap());
        BoardClient::new(board, RigId::new("test-rig"))
    }

    #[tokio::test]
    async fn list_tools_returns_four() {
        let client = make_board_client().await;
        let cancel = CancellationToken::new();
        let result = client.list_tools("s1", None, cancel).await.unwrap();
        assert_eq!(result.tools.len(), 4);
    }

    #[tokio::test]
    async fn read_board_empty() {
        let client = make_board_client().await;
        let result = handlers::handle_read_board(&client.board).await;
        let text = content_text(&result);
        assert!(text.contains("0 open"));
    }

    #[tokio::test]
    async fn create_and_claim_and_submit() {
        let client = make_board_client().await;

        // create
        let mut args = JsonObject::new();
        args.insert("title".into(), json!("test task"));
        let result = handlers::handle_create_task(&client.board, &client.rig_id, &args).await;
        let text = content_text(&result);
        assert!(text.contains("Created #1"));

        // claim
        let result = handlers::handle_claim_next(&client.board, &client.rig_id).await;
        let text = content_text(&result);
        assert!(text.contains("Claimed #1"));

        // submit
        let mut args = JsonObject::new();
        args.insert("item_id".into(), json!(1));
        let result = handlers::handle_submit(&client.board, &client.rig_id, &args).await;
        let text = content_text(&result);
        assert!(text.contains("Completed #1"));
    }

    #[tokio::test]
    async fn claim_empty_board() {
        let client = make_board_client().await;
        let result = handlers::handle_claim_next(&client.board, &client.rig_id).await;
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

    #[tokio::test]
    async fn handle_submit_missing_item_id_returns_error() {
        let client = make_board_client().await;
        let args = JsonObject::new();
        let result = handlers::handle_submit(&client.board, &client.rig_id, &args).await;
        let text = content_text(&result);
        assert!(text.contains("Missing item_id"));
    }

    #[tokio::test]
    async fn handle_create_task_missing_title_returns_error() {
        let client = make_board_client().await;
        let args = JsonObject::new();
        let result = handlers::handle_create_task(&client.board, &client.rig_id, &args).await;
        let text = content_text(&result);
        assert!(text.contains("Missing title"));
    }

    #[tokio::test]
    async fn handle_create_task_with_p0_and_p2_priorities() {
        let client = make_board_client().await;

        let mut args = JsonObject::new();
        args.insert("title".into(), json!("urgent task"));
        args.insert("priority".into(), json!("P0"));
        let result = handlers::handle_create_task(&client.board, &client.rig_id, &args).await;
        let text = content_text(&result);
        assert!(text.contains("P0"));

        let mut args = JsonObject::new();
        args.insert("title".into(), json!("low task"));
        args.insert("priority".into(), json!("P2"));
        let result = handlers::handle_create_task(&client.board, &client.rig_id, &args).await;
        let text = content_text(&result);
        assert!(text.contains("P2"));
    }

    fn test_ctx() -> ToolCallContext {
        ToolCallContext::new("test-session".into(), None, None)
    }

    #[tokio::test]
    async fn call_tool_unknown_returns_error() {
        let client = make_board_client().await;
        let cancel = CancellationToken::new();
        let ctx = test_ctx();
        let result = client
            .call_tool(&ctx, "unknown_tool", None, cancel)
            .await
            .unwrap();
        let text = content_text(&result);
        assert!(text.contains("Unknown tool"));
    }

    #[tokio::test]
    async fn call_tool_dispatch_all_known_tools() {
        let client = make_board_client().await;
        let cancel = CancellationToken::new();
        let ctx = test_ctx();

        // read_board via call_tool
        let result = client
            .call_tool(&ctx, "read_board", None, cancel.clone())
            .await
            .unwrap();
        assert!(content_text(&result).contains("open"));

        // claim_next via call_tool (empty board)
        let result = client
            .call_tool(&ctx, "claim_next", None, cancel.clone())
            .await
            .unwrap();
        assert!(content_text(&result).contains("No open items"));

        // create_task via call_tool
        let mut args = JsonObject::new();
        args.insert("title".into(), json!("dispatch task"));
        let result = client
            .call_tool(&ctx, "create_task", Some(args), cancel.clone())
            .await
            .unwrap();
        assert!(content_text(&result).contains("Created"));

        // submit via call_tool with missing item_id
        let result = client
            .call_tool(&ctx, "submit", None, cancel.clone())
            .await
            .unwrap();
        assert!(content_text(&result).contains("Missing item_id"));
    }

    #[tokio::test]
    async fn read_board_with_claimed_and_done_items() {
        let board = Arc::new(Board::in_memory().await.unwrap());
        let rig_id = RigId::new("reader-rig");
        let client = BoardClient::new(board.clone(), rig_id.clone());

        // Create two items
        let item1 = board
            .post(opengoose_board::work_item::PostWorkItem {
                title: "claimed item".into(),
                description: String::new(),
                created_by: rig_id.clone(),
                priority: opengoose_board::work_item::Priority::P1,
                tags: vec![],
            })
            .await
            .unwrap();
        let item2 = board
            .post(opengoose_board::work_item::PostWorkItem {
                title: "done item".into(),
                description: String::new(),
                created_by: rig_id.clone(),
                priority: opengoose_board::work_item::Priority::P2,
                tags: vec![],
            })
            .await
            .unwrap();

        // Claim one, submit the other
        board.claim(item1.id, &rig_id).await.unwrap();
        board.claim(item2.id, &rig_id).await.unwrap();
        board.submit(item2.id, &rig_id).await.unwrap();

        let result = handlers::handle_read_board(&client.board).await;
        let text = content_text(&result);
        assert!(text.contains("Claimed:"));
        assert!(text.contains("Recent done:"));
    }

    #[tokio::test]
    async fn get_info_returns_some() {
        let client = make_board_client().await;
        let info = client.get_info();
        assert!(info.is_some());
        assert_eq!(info.unwrap().server_info.name, "board");
    }

    #[tokio::test]
    async fn read_board_with_open_items_shows_open_section() {
        let board = Arc::new(Board::in_memory().await.unwrap());
        let rig_id = RigId::new("reader-rig");
        let client = BoardClient::new(board.clone(), rig_id.clone());

        // Post items but don't claim them — they stay Open
        board
            .post(opengoose_board::work_item::PostWorkItem {
                title: "open task one".into(),
                description: String::new(),
                created_by: rig_id.clone(),
                priority: opengoose_board::work_item::Priority::P0,
                tags: vec![],
            })
            .await
            .unwrap();
        board
            .post(opengoose_board::work_item::PostWorkItem {
                title: "open task two".into(),
                description: String::new(),
                created_by: rig_id.clone(),
                priority: opengoose_board::work_item::Priority::P1,
                tags: vec![],
            })
            .await
            .unwrap();

        let result = handlers::handle_read_board(&client.board).await;
        let text = content_text(&result);
        assert!(text.contains("2 open"), "expected 2 open items: {text}");
        assert!(text.contains("Open:"), "expected Open section: {text}");
        assert!(text.contains("open task one"));
    }

    #[test]
    fn content_text_ignores_non_text_content() {
        use rmcp::model::Content;
        let image_content = Content::image("base64data", "image/png");
        let result = CallToolResult::success(vec![image_content]);
        assert_eq!(content_text(&result), "");
    }

    #[tokio::test]
    async fn handle_submit_nonexistent_item_returns_error() {
        let client = make_board_client().await;
        let mut args = JsonObject::new();
        args.insert("item_id".into(), json!(999));
        let result = handlers::handle_submit(&client.board, &client.rig_id, &args).await;
        let text = content_text(&result);
        assert!(text.contains("Submit failed"), "expected error: {text}");
    }
}
