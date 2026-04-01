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
            "board__remember" => handlers::handle_remember(&self.board, &self.rig_id, &args).await,
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
        let board = Arc::new(
            Board::in_memory()
                .await
                .expect("in-memory board should initialize"),
        );
        BoardClient::new(board, RigId::new("test-rig"))
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

    fn test_ctx() -> ToolCallContext {
        ToolCallContext::new("test-session".into(), None, None)
    }

    #[tokio::test]
    async fn list_tools_via_mcp_trait() {
        let client = make_board_client().await;
        let cancel = CancellationToken::new();
        let result = client
            .list_tools("s1", None, cancel)
            .await
            .expect("async operation should succeed");
        assert_eq!(result.tools.len(), 5);
    }

    #[tokio::test]
    async fn call_tool_unknown_returns_error() {
        let client = make_board_client().await;
        let cancel = CancellationToken::new();
        let ctx = test_ctx();
        let result = client
            .call_tool(&ctx, "unknown_tool", None, cancel)
            .await
            .expect("call_tool should succeed");
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
            .expect("call_tool should succeed");
        assert!(content_text(&result).contains("open"));

        // claim_next via call_tool (empty board)
        let result = client
            .call_tool(&ctx, "claim_next", None, cancel.clone())
            .await
            .expect("call_tool should succeed");
        assert!(content_text(&result).contains("No open items"));

        // create_task via call_tool
        let mut args = JsonObject::new();
        args.insert("title".into(), json!("dispatch task"));
        let result = client
            .call_tool(&ctx, "create_task", Some(args), cancel.clone())
            .await
            .expect("call_tool should succeed");
        assert!(content_text(&result).contains("Created"));

        // submit via call_tool with missing item_id
        let result = client
            .call_tool(&ctx, "submit", None, cancel.clone())
            .await
            .expect("call_tool should succeed");
        assert!(content_text(&result).contains("Missing item_id"));
    }

    #[tokio::test]
    async fn get_info_returns_some() {
        let client = make_board_client().await;
        let info = client.get_info();
        assert!(info.is_some());
        assert_eq!(
            info.expect("server_info should be present")
                .server_info
                .name,
            "board"
        );
    }
}
