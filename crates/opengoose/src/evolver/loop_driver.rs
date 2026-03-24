// Evolver main loop — lazy Agent init, stamp_notify listener, fallback sweep.

use super::{EVOLVER_SYSTEM_PROMPT, FALLBACK_SWEEP_SECS, LOW_STAMP_THRESHOLD, RealAgentCaller};
use crate::runtime::{AgentConfig, create_agent};
use goose::agents::Agent;
use opengoose_board::Board;
use std::sync::Arc;
use tokio::sync::Notify;
use tracing::{info, warn};

/// Evolver run loop. Lazy-inits Agent on first stamp event.
pub async fn run(board: Arc<Board>, stamp_notify: Arc<Notify>) {
    info!("evolver: listening for stamp events");

    let mut agent: Option<Agent> = None;

    loop {
        // Wait for stamp_notify OR fallback sweep
        tokio::select! {
            _ = stamp_notify.notified() => {}
            _ = tokio::time::sleep(std::time::Duration::from_secs(FALLBACK_SWEEP_SECS)) => {}
        }

        // Query unprocessed low stamps
        let stamps = match board.unprocessed_low_stamps(LOW_STAMP_THRESHOLD).await {
            Ok(s) => s,
            Err(e) => {
                warn!("evolver: failed to query stamps: {e}");
                continue;
            }
        };

        if stamps.is_empty() {
            // Idle-time sweep: re-evaluate dormant skills once per hour
            if let Some(ref agent) = agent {
                use std::sync::atomic::{AtomicU64, Ordering};
                static LAST_SWEEP_EPOCH: AtomicU64 = AtomicU64::new(0);

                let now_epoch = std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .unwrap_or_default()
                    .as_secs();
                let last = LAST_SWEEP_EPOCH.load(Ordering::Relaxed);

                if now_epoch - last >= 3600 {
                    LAST_SWEEP_EPOCH.store(now_epoch, Ordering::Relaxed);
                    info!("evolver: running idle-time sweep");
                    let caller = RealAgentCaller { agent };
                    if let Err(e) = super::sweep::run_sweep(&board, &caller).await {
                        warn!("evolver: sweep failed: {e}");
                    }
                }
            }
            continue;
        }

        // Lazy init Agent on first real work
        if agent.is_none() {
            match create_agent(AgentConfig {
                session_id: "evolver".into(),
                system_prompt: Some(EVOLVER_SYSTEM_PROMPT.into()),
            })
            .await
            {
                Ok(a) => {
                    info!("evolver: agent initialized");
                    agent = Some(a);
                }
                Err(e) => {
                    warn!("evolver: failed to create agent: {e}");
                    continue;
                }
            }
        }

        for stamp in &stamps {
            // Atomically mark as evolved (prevents duplicate processing)
            match board.mark_stamp_evolved(stamp.id).await {
                Ok(true) => {}
                Ok(false) => continue, // another Evolver got it
                Err(e) => {
                    warn!("evolver: failed to mark stamp {}: {e}", stamp.id);
                    continue;
                }
            }

            let caller = RealAgentCaller {
                agent: agent
                    .as_ref()
                    .expect("agent initialized above or loop continued"),
            };
            if let Err(e) = super::pipeline::process_stamp(&board, &caller, stamp).await {
                warn!("evolver: failed to process stamp {}: {e}", stamp.id);
            }
        }
    }
}
