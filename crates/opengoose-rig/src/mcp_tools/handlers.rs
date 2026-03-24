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
