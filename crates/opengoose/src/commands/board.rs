// Board CLI command handlers

use crate::cli::BoardAction;
use anyhow::Result;
use opengoose_board::Board;
use opengoose_board::work_item::{PostWorkItem, Priority, RigId, Status};

pub async fn run_board_command(board: &Board, action: BoardAction) -> Result<()> {
    let rig_id = RigId::new("cli");

    match action {
        BoardAction::Status => {
            show_board(board).await?;
        }
        BoardAction::Ready => {
            let items = board.ready().await?;
            if items.is_empty() {
                println!("No claimable items.");
            } else {
                for item in &items {
                    println!("#{} {:?} \"{}\"", item.id, item.priority, item.title);
                }
            }
        }
        BoardAction::Claim { id } => {
            let item = board.claim(id, &rig_id).await?;
            println!("Claimed #{}: \"{}\"", item.id, item.title);
        }
        BoardAction::Submit { id } => {
            let item = board.submit(id, &rig_id).await?;
            println!("Completed #{}: \"{}\"", item.id, item.title);
        }
        BoardAction::Create {
            title,
            priority,
            tags,
            parent,
        } => {
            let priority = Priority::parse(&priority).unwrap_or_default();
            let item = board
                .post(PostWorkItem {
                    title,
                    description: String::new(),
                    created_by: rig_id,
                    priority,
                    tags,
                    parent_id: parent,
                })
                .await?;
            println!(
                "Created #{}: \"{}\" ({:?})",
                item.id, item.title, item.priority
            );
        }
        BoardAction::Children { id } => {
            let children = board.children(id).await?;
            if children.is_empty() {
                println!("No sub-tasks for #{id}");
            } else {
                println!("Sub-tasks for #{id}:");
                for child in &children {
                    println!(
                        "  #{} {:?} [{}] \"{}\"",
                        child.id, child.priority, child.status, child.title
                    );
                }
            }
        }
        BoardAction::Abandon { id } => {
            let item = board.abandon(id).await?;
            println!("Abandoned #{}: \"{}\"", item.id, item.title);
        }
        BoardAction::Stamp {
            id,
            quality,
            reliability,
            helpfulness,
            severity,
            comment,
        } => {
            let stamped_by = "human";
            // 작업의 claimed_by가 target rig
            let item = board
                .get(id)
                .await?
                .ok_or_else(|| anyhow::anyhow!("item not found"))?;
            let target_rig = item.claimed_by.as_ref().unwrap_or(&item.created_by);
            let target = target_rig.as_ref();

            let comment_ref = comment.as_deref();
            for (dim, score) in [
                ("Quality", quality),
                ("Reliability", reliability),
                ("Helpfulness", helpfulness),
            ] {
                board
                    .add_stamp(opengoose_board::AddStampParams {
                        target_rig: target,
                        work_item_id: id,
                        dimension: dim,
                        score,
                        severity: &severity,
                        stamped_by,
                        comment: comment_ref,
                        active_skill_versions: None,
                    })
                    .await?;
            }

            let trust = board.trust_level(target_rig).await?;
            let pts = board.weighted_score(target_rig).await?;
            println!(
                "Stamped #{id} (target: {target}): q:{quality} r:{reliability} h:{helpfulness} {severity}"
            );
            if let Some(c) = &comment {
                println!("  comment: {c}");
            }
            println!("  {target}: {trust} ({pts:.1}pts)");

            // Evolver run loop handles skill generation from low stamps asynchronously
        }
    }

    Ok(())
}

pub async fn show_board(board: &Board) -> Result<()> {
    let items = board.list().await?;

    let open: Vec<_> = items.iter().filter(|i| i.status == Status::Open).collect();
    let claimed: Vec<_> = items
        .iter()
        .filter(|i| i.status == Status::Claimed)
        .collect();
    let done: Vec<_> = items.iter().filter(|i| i.status == Status::Done).collect();

    println!(
        "Board: {} open · {} claimed · {} done",
        open.len(),
        claimed.len(),
        done.len()
    );

    if !open.is_empty() {
        println!("\nOpen:");
        for item in &open {
            println!("  ○ #{} {:?} \"{}\"", item.id, item.priority, item.title);
        }
    }

    if !claimed.is_empty() {
        println!("\nClaimed:");
        for item in &claimed {
            let by = item
                .claimed_by
                .as_ref()
                .map(|r| r.0.as_str())
                .unwrap_or("?");
            println!("  ● #{} \"{}\" (by {})", item.id, item.title, by);
        }
    }

    if !done.is_empty() {
        println!("\nDone (recent):");
        for item in done.iter().rev().take(5) {
            println!("  ✓ #{} \"{}\"", item.id, item.title);
        }
    }

    Ok(())
}
