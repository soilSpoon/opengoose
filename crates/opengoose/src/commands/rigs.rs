// Rigs CLI command handler

use crate::cli::RigsAction;
use anyhow::Result;
use opengoose_board::Board;

pub async fn run_rigs_command(board: &Board, action: Option<RigsAction>) -> Result<()> {
    match action {
        None => {
            // opengoose rigs — 목록 표시
            let rigs = board.list_rigs().await?;
            if rigs.is_empty() {
                println!("No rigs registered.");
            } else {
                for rig in &rigs {
                    let tags = rig.tags.as_deref().unwrap_or("[]");
                    let recipe = rig.recipe.as_deref().unwrap_or("-");
                    println!(
                        "  {}  {}  recipe:{}  tags:{}",
                        rig.id, rig.rig_type, recipe, tags
                    );
                }
            }
        }
        Some(RigsAction::Add { id, recipe, tags }) => {
            let tags = if tags.is_empty() {
                None
            } else {
                Some(tags.as_slice())
            };
            board.register_rig(&id, "ai", Some(&recipe), tags).await?;
            println!("Registered {id} (recipe: {recipe})");
        }
        Some(RigsAction::Remove { id }) => {
            board.remove_rig(&id).await?;
            println!("Removed {id}");
        }
        Some(RigsAction::Trust { id }) => {
            let pts = board.weighted_score(&id).await?;
            let level = board.trust_level(&id).await?;
            println!("{id}: {level} ({pts:.1}pts)");
        }
    }
    Ok(())
}
