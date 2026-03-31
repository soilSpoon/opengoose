use futures::StreamExt;
use goose::agents::AgentEvent;
use goose::conversation::message::MessageContent;
use opengoose_board::Board;
use opengoose_board::work_item::Status;
use opengoose_rig::rig::Operator;
use std::sync::Arc;
use tokio::sync::mpsc;
use tracing::warn;

use super::AgentMsg;
use crate::tui::app::{App, RigInfo, RigStatus};

/// Spawn Operator.chat_streaming() as a separate tokio task, streaming results via channel.
pub fn spawn_operator_reply(operator: Arc<Operator>, input: String, tx: mpsc::Sender<AgentMsg>) {
    tokio::spawn(async move {
        match operator.chat_streaming(&input).await {
            Ok(stream) => {
                tokio::pin!(stream);
                while let Some(event) = stream.next().await {
                    match event {
                        Ok(AgentEvent::Message(msg))
                            if msg.role == rmcp::model::Role::Assistant =>
                        {
                            for content in &msg.content {
                                if let MessageContent::Text(text) = content
                                    && let Err(e) = tx.send(AgentMsg::Text(text.text.clone())).await
                                {
                                    warn!("agent text channel closed: {e}");
                                }
                            }
                        }
                        Err(e) => {
                            if let Err(send_err) = tx
                                .send(AgentMsg::Text(format!("\n⚠ Stream error: {e}")))
                                .await
                            {
                                warn!("agent error channel closed: {send_err}");
                            }
                            break;
                        }
                        _ => {}
                    }
                }
            }
            Err(e) => {
                if let Err(send_err) = tx.send(AgentMsg::Text(format!("Error: {e}"))).await {
                    warn!("agent error channel closed: {send_err}");
                }
            }
        }
        if let Err(e) = tx.send(AgentMsg::Done).await {
            warn!("agent done channel closed: {e}");
        }
    });
}

/// Load rig info from the board and update app state.
pub async fn load_rigs(board: &Board, app: &mut App) {
    if let Ok(rigs) = board.list_rigs().await {
        let mut infos = Vec::new();
        for rig in &rigs {
            let rig_id = opengoose_board::RigId::new(&rig.id);
            let trust = board.trust_level(&rig_id).await.unwrap_or("L1");
            let is_working = app.board.items.iter().any(|i| {
                i.status == Status::Claimed && i.claimed_by.as_ref().is_some_and(|r| r.0 == rig.id)
            });

            infos.push(RigInfo {
                id: rig.id.clone(),
                trust_level: trust.to_string(),
                status: if is_working {
                    RigStatus::Working
                } else {
                    RigStatus::Idle
                },
            });
        }
        app.board.rigs = infos;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn load_rigs_marks_working_status_from_board_snapshot() {
        let mut app = App::new();
        let board = std::sync::Arc::new(
            opengoose_board::Board::in_memory()
                .await
                .expect("in-memory board should initialize"),
        );
        board
            .register_rig("r1", "ai", Some("worker"), Some(&["tag".into()]))
            .await
            .expect("register_rig should succeed");
        let item = board
            .post(opengoose_board::work_item::PostWorkItem {
                title: "Active".into(),
                description: String::new(),
                created_by: opengoose_board::work_item::RigId::new("creator"),
                priority: opengoose_board::work_item::Priority::P1,
                tags: vec![],
                parent_id: None,
            })
            .await
            .expect("board operation should succeed");
        board
            .claim(item.id, &opengoose_board::work_item::RigId::new("r1"))
            .await
            .expect("claim should succeed");
        app.board.items = board.list().await.expect("list should succeed");

        load_rigs(&board, &mut app).await;

        let r1 = app
            .board
            .rigs
            .iter()
            .find(|r| r.id == "r1")
            .expect("r1 not found");
        assert_eq!(r1.status.icon(), "⚙");
    }

    #[tokio::test]
    async fn load_rigs_with_no_claimed_items_all_rigs_are_idle() {
        let mut app = App::new();
        let board = std::sync::Arc::new(
            opengoose_board::Board::in_memory()
                .await
                .expect("in-memory board should initialize"),
        );

        app.board.items = board.list().await.expect("list should succeed");
        load_rigs(&board, &mut app).await;

        for rig in &app.board.rigs {
            assert!(
                matches!(rig.status, RigStatus::Idle),
                "expected idle for rig {}",
                rig.id
            );
        }
    }

    #[tokio::test]
    async fn load_rigs_registered_rig_without_claimed_item_is_idle() {
        let mut app = App::new();
        let board = std::sync::Arc::new(
            opengoose_board::Board::in_memory()
                .await
                .expect("in-memory board should initialize"),
        );

        board
            .register_rig("worker42", "ai", Some("worker"), Some(&["tag".into()]))
            .await
            .expect("register_rig should succeed");
        app.board.items = board.list().await.expect("list should succeed");
        load_rigs(&board, &mut app).await;

        let w = app
            .board
            .rigs
            .iter()
            .find(|r| r.id == "worker42")
            .expect("worker42 should appear");
        assert!(matches!(w.status, RigStatus::Idle));
    }
}
