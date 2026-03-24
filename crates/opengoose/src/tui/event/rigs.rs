use futures::StreamExt;
use goose::agents::AgentEvent;
use goose::conversation::message::MessageContent;
use opengoose_board::Board;
use opengoose_board::work_item::Status;
use opengoose_rig::rig::Operator;
use std::sync::Arc;
use tokio::sync::mpsc;

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
                                if let MessageContent::Text(text) = content {
                                    let _ = tx.send(AgentMsg::Text(text.text.clone())).await;
                                }
                            }
                        }
                        Err(e) => {
                            let _ = tx
                                .send(AgentMsg::Text(format!("\n⚠ Stream error: {e}")))
                                .await;
                            break;
                        }
                        _ => {}
                    }
                }
            }
            Err(e) => {
                let _ = tx.send(AgentMsg::Text(format!("Error: {e}"))).await;
            }
        }
        let _ = tx.send(AgentMsg::Done).await;
    });
}

/// Load rig info from DB and update app state.
pub async fn load_rigs(board: &Board, app: &mut App) {
    if let Ok(rigs) = board.list_rigs().await {
        let mut infos = Vec::new();
        for rig in &rigs {
            let trust = board.trust_level(&rig.id).await.unwrap_or("L1");
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
