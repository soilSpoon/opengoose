// Headless mode — post a task to the Board and wait for Worker completion

use anyhow::Result;
use opengoose_board::Board;
use opengoose_board::work_item::{PostWorkItem, Priority, RigId, Status};

/// Post a task to the Board and poll until the Worker completes or abandons it.
/// Times out after 10 minutes.
pub async fn run_headless(board: &Board, task: &str) -> Result<()> {
    let rig_id = RigId::new("headless");
    let item = board
        .post(PostWorkItem {
            title: task.to_string(),
            description: String::new(),
            created_by: rig_id,
            priority: Priority::P1,
            tags: vec![],
        })
        .await?;

    println!(
        "Posted #{}: \"{}\" — waiting for Worker...",
        item.id, item.title
    );

    let timeout = tokio::time::sleep(std::time::Duration::from_secs(600));
    tokio::pin!(timeout);

    loop {
        let notify = board.notify_handle();
        let notified = notify.notified();

        match board.get(item.id).await? {
            Some(wi) if wi.status == Status::Done => {
                println!("✓ #{} completed", item.id);
                break;
            }
            Some(wi) if wi.status == Status::Abandoned => {
                anyhow::bail!("work item #{} was abandoned", item.id);
            }
            Some(_) => {}
            None => anyhow::bail!("work item #{} was deleted", item.id),
        }

        tokio::select! {
            _ = notified => {}
            _ = &mut timeout => anyhow::bail!("timed out waiting for work item #{} (10 min)", item.id),
            _ = tokio::signal::ctrl_c() => {
                eprintln!("\nInterrupted.");
                return Ok(());
            }
        }
    }

    Ok(())
}
