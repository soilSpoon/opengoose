//! WorkerPool — manages running Worker runtimes dynamically.

use anyhow::Result;
use opengoose_board::Board;
use opengoose_board::work_item::RigId;
use opengoose_rig::pipeline::Middleware;
use opengoose_rig::rig::Worker;
use serde::Serialize;
use std::collections::HashMap;
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};
use tokio::sync::RwLock;
use tokio::task::JoinHandle;
use tracing::info;

struct WorkerHandle {
    worker: Arc<Worker>,
    join_handle: JoinHandle<()>,
}

#[derive(Debug, Default)]
pub struct WorkerConfig {
    pub recipe: Option<String>,
    pub model: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct WorkerInfo {
    pub id: String,
    pub status: &'static str,
}

pub struct WorkerPool {
    handles: RwLock<HashMap<String, WorkerHandle>>,
    board: Arc<Board>,
    middleware: Vec<Arc<dyn Middleware>>,
    counter: AtomicU64,
}

impl WorkerPool {
    pub fn new(board: Arc<Board>, middleware: Vec<Arc<dyn Middleware>>) -> Self {
        Self {
            handles: RwLock::new(HashMap::new()),
            board,
            middleware,
            counter: AtomicU64::new(1),
        }
    }

    pub async fn spawn(&self, id: Option<String>, config: WorkerConfig) -> Result<String> {
        let worker_id = id.unwrap_or_else(|| {
            let n = self.counter.fetch_add(1, Ordering::Relaxed);
            format!("worker-{n}")
        });

        if self.handles.read().await.contains_key(&worker_id) {
            anyhow::bail!("worker '{}' already exists", worker_id);
        }

        let agent = create_worker_agent_with_config(&config).await?;
        let rig_id = RigId::new(&worker_id);

        let worker = Arc::new(Worker::new(
            rig_id,
            Arc::clone(&self.board),
            agent,
            opengoose_rig::work_mode::TaskMode,
            self.middleware.clone(),
        ));

        let worker_handle = Arc::clone(&worker);
        let join_handle = tokio::spawn(async move { worker_handle.run().await });

        self.handles.write().await.insert(
            worker_id.clone(),
            WorkerHandle {
                worker,
                join_handle,
            },
        );

        info!(id = %worker_id, "worker spawned");
        Ok(worker_id)
    }

    pub async fn remove(&self, id: &str) -> Result<()> {
        let handle = self
            .handles
            .write()
            .await
            .remove(id)
            .ok_or_else(|| anyhow::anyhow!("worker '{}' not found", id))?;

        handle.worker.cancel();
        let _ = tokio::time::timeout(std::time::Duration::from_secs(10), handle.join_handle).await;
        info!(id = %id, "worker removed");
        Ok(())
    }

    pub async fn list(&self) -> Vec<WorkerInfo> {
        self.handles
            .read()
            .await
            .keys()
            .map(|id| WorkerInfo {
                id: id.clone(),
                status: "running",
            })
            .collect()
    }

    pub async fn cancel_all(&self) {
        let handles: Vec<_> = self.handles.write().await.drain().collect();
        for (id, handle) in handles {
            handle.worker.cancel();
            let _ =
                tokio::time::timeout(std::time::Duration::from_secs(5), handle.join_handle).await;
            info!(id = %id, "worker cancelled");
        }
    }

    pub async fn len(&self) -> usize {
        self.handles.read().await.len()
    }
}

async fn create_worker_agent_with_config(config: &WorkerConfig) -> Result<goose::agents::Agent> {
    if let Some(ref recipe) = config.recipe {
        info!(recipe = %recipe, "worker recipe requested (not yet implemented)");
    }

    let prev_model = config.model.as_ref().map(|m| {
        let prev = std::env::var("GOOSE_MODEL").ok();
        unsafe { std::env::set_var("GOOSE_MODEL", m) };
        prev
    });

    let result = crate::runtime::create_worker_agent().await;

    if let Some(prev) = prev_model {
        unsafe {
            match prev {
                Some(v) => std::env::set_var("GOOSE_MODEL", v),
                None => std::env::remove_var("GOOSE_MODEL"),
            }
        }
    }

    result.map(|(agent, _)| agent)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn pool_starts_empty() {
        let board = Arc::new(Board::in_memory().await.expect("board"));
        let pool = WorkerPool::new(board, vec![]);
        assert_eq!(pool.len().await, 0);
        assert!(pool.list().await.is_empty());
    }

    #[tokio::test]
    async fn remove_nonexistent_returns_error() {
        let board = Arc::new(Board::in_memory().await.expect("board"));
        let pool = WorkerPool::new(board, vec![]);
        assert!(pool.remove("ghost").await.is_err());
    }

    #[tokio::test]
    async fn cancel_all_on_empty_pool_is_noop() {
        let board = Arc::new(Board::in_memory().await.expect("board"));
        let pool = WorkerPool::new(board, vec![]);
        pool.cancel_all().await;
        assert_eq!(pool.len().await, 0);
    }
}
