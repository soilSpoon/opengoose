// Handler functions — board tool execution logic

use opengoose_board::work_item::{PostWorkItem, Priority, RigId};
use opengoose_board::{Board, Status};
use rmcp::model::{CallToolResult, Content, JsonObject};
use serde_json::Value;
use std::sync::Arc;

pub async fn handle_read_board(board: &Arc<Board>) -> CallToolResult {
    let items = match board.list().await {
        Ok(items) => items,
        Err(e) => {
            return CallToolResult::error(vec![Content::text(format!("Board error: {e}"))]);
        }
    };

    let open: Vec<_> = items.iter().filter(|i| i.status == Status::Open).collect();
    let claimed: Vec<_> = items
        .iter()
        .filter(|i| i.status == Status::Claimed)
        .collect();
    let done: Vec<_> = items.iter().filter(|i| i.status == Status::Done).collect();

    let mut text = format!(
        "Board: {} open, {} claimed, {} done\n",
        open.len(),
        claimed.len(),
        done.len()
    );

    if !open.is_empty() {
        text.push_str("\nOpen:\n");
        for item in &open {
            text.push_str(&format!(
                "  #{} {:?} \"{}\"\n",
                item.id, item.priority, item.title
            ));
        }
    }

    if !claimed.is_empty() {
        text.push_str("\nClaimed:\n");
        for item in &claimed {
            let by = item
                .claimed_by
                .as_ref()
                .map(|r| r.0.as_str())
                .unwrap_or("?");
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

pub async fn handle_claim_next(board: &Arc<Board>, rig_id: &RigId) -> CallToolResult {
    let ready = match board.ready().await {
        Ok(items) => items,
        Err(e) => {
            return CallToolResult::error(vec![Content::text(format!("Board error: {e}"))]);
        }
    };

    let Some(item) = ready.first() else {
        return CallToolResult::success(vec![Content::text("No open items available.")]);
    };

    let item_id = item.id;
    match board.claim(item_id, rig_id).await {
        Ok(claimed) => CallToolResult::success(vec![Content::text(format!(
            "Claimed #{}: \"{}\" ({:?})",
            claimed.id, claimed.title, claimed.priority
        ))]),
        Err(e) => CallToolResult::error(vec![Content::text(format!("Claim failed: {e}"))]),
    }
}

pub async fn handle_submit(board: &Arc<Board>, rig_id: &RigId, args: &JsonObject) -> CallToolResult {
    let Some(item_id) = args.get("item_id").and_then(Value::as_i64) else {
        return CallToolResult::error(vec![Content::text("Missing item_id")]);
    };

    match board.submit(item_id, rig_id).await {
        Ok(done) => CallToolResult::success(vec![Content::text(format!(
            "Completed #{}: \"{}\"",
            done.id, done.title
        ))]),
        Err(e) => CallToolResult::error(vec![Content::text(format!("Submit failed: {e}"))]),
    }
}

pub async fn handle_create_task(
    board: &Arc<Board>,
    rig_id: &RigId,
    args: &JsonObject,
) -> CallToolResult {
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

    match board
        .post(PostWorkItem {
            title: title.to_string(),
            description,
            created_by: rig_id.clone(),
            priority,
            tags: vec![],
        })
        .await
    {
        Ok(item) => CallToolResult::success(vec![Content::text(format!(
            "Created #{}: \"{}\" ({:?})",
            item.id, item.title, item.priority
        ))]),
        Err(e) => CallToolResult::error(vec![Content::text(format!("Create failed: {e}"))]),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use opengoose_board::Board;
    use serde_json::json;

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
    async fn read_board_empty() {
        let board = Arc::new(Board::in_memory().await.unwrap());
        let result = handle_read_board(&board).await;
        let text = content_text(&result);
        assert!(text.contains("0 open"));
    }

    #[tokio::test]
    async fn create_and_claim_and_submit() {
        let board = Arc::new(Board::in_memory().await.unwrap());
        let rig_id = RigId::new("test-rig");

        // create
        let mut args = JsonObject::new();
        args.insert("title".into(), json!("test task"));
        let result = handle_create_task(&board, &rig_id, &args).await;
        let text = content_text(&result);
        assert!(text.contains("Created #1"));

        // claim
        let result = handle_claim_next(&board, &rig_id).await;
        let text = content_text(&result);
        assert!(text.contains("Claimed #1"));

        // submit
        let mut args = JsonObject::new();
        args.insert("item_id".into(), json!(1));
        let result = handle_submit(&board, &rig_id, &args).await;
        let text = content_text(&result);
        assert!(text.contains("Completed #1"));
    }

    #[tokio::test]
    async fn claim_empty_board() {
        let board = Arc::new(Board::in_memory().await.unwrap());
        let rig_id = RigId::new("test-rig");
        let result = handle_claim_next(&board, &rig_id).await;
        let text = content_text(&result);
        assert!(text.contains("No open items"));
    }

    #[tokio::test]
    async fn handle_submit_missing_item_id_returns_error() {
        let board = Arc::new(Board::in_memory().await.unwrap());
        let rig_id = RigId::new("test-rig");
        let args = JsonObject::new();
        let result = handle_submit(&board, &rig_id, &args).await;
        let text = content_text(&result);
        assert!(text.contains("Missing item_id"));
    }

    #[tokio::test]
    async fn handle_create_task_missing_title_returns_error() {
        let board = Arc::new(Board::in_memory().await.unwrap());
        let rig_id = RigId::new("test-rig");
        let args = JsonObject::new();
        let result = handle_create_task(&board, &rig_id, &args).await;
        let text = content_text(&result);
        assert!(text.contains("Missing title"));
    }

    #[tokio::test]
    async fn handle_create_task_with_p0_and_p2_priorities() {
        let board = Arc::new(Board::in_memory().await.unwrap());
        let rig_id = RigId::new("test-rig");

        let mut args = JsonObject::new();
        args.insert("title".into(), json!("urgent task"));
        args.insert("priority".into(), json!("P0"));
        let result = handle_create_task(&board, &rig_id, &args).await;
        let text = content_text(&result);
        assert!(text.contains("P0"));

        let mut args = JsonObject::new();
        args.insert("title".into(), json!("low task"));
        args.insert("priority".into(), json!("P2"));
        let result = handle_create_task(&board, &rig_id, &args).await;
        let text = content_text(&result);
        assert!(text.contains("P2"));
    }

    #[tokio::test]
    async fn read_board_with_claimed_and_done_items() {
        let board = Arc::new(Board::in_memory().await.unwrap());
        let rig_id = RigId::new("reader-rig");

        // Create two items
        let item1 = board
            .post(PostWorkItem {
                title: "claimed item".into(),
                description: String::new(),
                created_by: rig_id.clone(),
                priority: Priority::P1,
                tags: vec![],
            })
            .await
            .unwrap();
        let item2 = board
            .post(PostWorkItem {
                title: "done item".into(),
                description: String::new(),
                created_by: rig_id.clone(),
                priority: Priority::P2,
                tags: vec![],
            })
            .await
            .unwrap();

        // Claim one, submit the other
        board.claim(item1.id, &rig_id).await.unwrap();
        board.claim(item2.id, &rig_id).await.unwrap();
        board.submit(item2.id, &rig_id).await.unwrap();

        let result = handle_read_board(&board).await;
        let text = content_text(&result);
        assert!(text.contains("Claimed:"));
        assert!(text.contains("Recent done:"));
    }

    #[tokio::test]
    async fn read_board_with_open_items_shows_open_section() {
        let board = Arc::new(Board::in_memory().await.unwrap());
        let rig_id = RigId::new("reader-rig");

        // Post items but don't claim them — they stay Open
        board
            .post(PostWorkItem {
                title: "open task one".into(),
                description: String::new(),
                created_by: rig_id.clone(),
                priority: Priority::P0,
                tags: vec![],
            })
            .await
            .unwrap();
        board
            .post(PostWorkItem {
                title: "open task two".into(),
                description: String::new(),
                created_by: rig_id.clone(),
                priority: Priority::P1,
                tags: vec![],
            })
            .await
            .unwrap();

        let result = handle_read_board(&board).await;
        let text = content_text(&result);
        assert!(text.contains("2 open"), "expected 2 open items: {text}");
        assert!(text.contains("Open:"), "expected Open section: {text}");
        assert!(text.contains("open task one"));
    }

    #[tokio::test]
    async fn handle_submit_nonexistent_item_returns_error() {
        let board = Arc::new(Board::in_memory().await.unwrap());
        let rig_id = RigId::new("test-rig");
        let mut args = JsonObject::new();
        args.insert("item_id".into(), json!(999));
        let result = handle_submit(&board, &rig_id, &args).await;
        let text = content_text(&result);
        assert!(text.contains("Submit failed"), "expected error: {text}");
    }

    #[test]
    fn content_text_ignores_non_text_content() {
        use rmcp::model::Content;
        let image_content = Content::image("base64data", "image/png");
        let result = CallToolResult::success(vec![image_content]);
        assert_eq!(content_text(&result), "");
    }
}
