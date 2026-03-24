use opengoose_board::Board;
use opengoose_board::work_item::{PostWorkItem, Priority, RigId};
use opengoose_rig::rig::Operator;
use std::sync::Arc;
use tokio::sync::mpsc;

use super::rigs::spawn_operator_reply;
use super::AgentMsg;
use crate::tui::app::{App, ChatLine};

/// Parsed TUI command from user input.
#[derive(Debug, PartialEq, Eq)]
pub enum Command<'a> {
    Board,
    Task(&'a str),
    TaskUsage,
    Quit,
    Chat(&'a str),
}

/// Pure parsing: extract command from user input text.
pub fn parse_command(input: &str) -> Command<'_> {
    if input == "/board" {
        return Command::Board;
    }

    if input == "/task" {
        return Command::TaskUsage;
    }

    if let Some(task_title) = input.strip_prefix("/task ") {
        let task_title = task_title.trim().trim_matches('"');
        if task_title.is_empty() {
            return Command::TaskUsage;
        }
        return Command::Task(task_title);
    }

    if input == "/quit" || input == "/q" {
        return Command::Quit;
    }

    Command::Chat(input)
}

/// Handle user input: dispatch based on parsed command.
pub async fn handle_input(
    app: &mut App,
    text: &str,
    agent_tx: &mpsc::Sender<AgentMsg>,
    board: &Arc<Board>,
    operator: &Arc<Operator>,
) {
    match parse_command(text) {
        Command::Board => {
            if let Ok(items) = board.list().await {
                app.board.items = items.clone();
                let (open, claimed, done) = app.board_summary();
                app.push_chat(ChatLine::System(format!(
                    "Board: {open} open · {claimed} claimed · {done} done"
                )));
            }
        }
        Command::TaskUsage => {
            app.push_chat(ChatLine::System("Usage: /task \"description\"".into()));
        }
        Command::Task(title) => {
            handle_task(app, title, board).await;
        }
        Command::Quit => {
            app.should_quit = true;
        }
        Command::Chat(_) => {
            if app.agent_busy {
                app.push_chat(ChatLine::System("Agent is busy...".into()));
                return;
            }
            app.agent_busy = true;
            spawn_operator_reply(operator.clone(), text.to_string(), agent_tx.clone());
        }
    }
}

/// /task: post to Board for Worker to pick up.
async fn handle_task(app: &mut App, title: &str, board: &Arc<Board>) {
    match board
        .post(PostWorkItem {
            title: title.to_string(),
            description: String::new(),
            created_by: RigId::new("operator"),
            priority: Priority::P1,
            tags: vec![],
        })
        .await
    {
        Ok(item) => {
            app.push_chat(ChatLine::System(format!(
                "● #{} \"{}\" — posted (Worker will pick it up)",
                item.id, item.title
            )));
            if let Ok(items) = board.list().await {
                app.board.items = items;
            }
        }
        Err(e) => {
            app.push_chat(ChatLine::System(format!("Post failed: {e}")));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_command_board() {
        assert_eq!(parse_command("/board"), Command::Board);
    }

    #[test]
    fn parse_command_quit_variants() {
        assert_eq!(parse_command("/quit"), Command::Quit);
        assert_eq!(parse_command("/q"), Command::Quit);
    }

    #[test]
    fn parse_command_task_usage() {
        assert_eq!(parse_command("/task"), Command::TaskUsage);
        assert_eq!(parse_command("/task \"\""), Command::TaskUsage);
    }

    #[test]
    fn parse_command_task_with_title() {
        assert_eq!(parse_command("/task \"implement feature\""), Command::Task("implement feature"));
        assert_eq!(parse_command("/task do stuff"), Command::Task("do stuff"));
    }

    #[test]
    fn parse_command_chat() {
        assert_eq!(parse_command("hello world"), Command::Chat("hello world"));
        assert_eq!(parse_command("/unknown"), Command::Chat("/unknown"));
    }
}
