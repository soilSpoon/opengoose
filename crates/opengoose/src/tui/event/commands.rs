use opengoose_board::Board;
use opengoose_board::work_item::{PostWorkItem, Priority, RigId};
use opengoose_rig::rig::Operator;
use std::sync::Arc;
use tokio::sync::mpsc;

use super::AgentMsg;
use super::rigs::spawn_operator_reply;
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
    use super::super::keys::handle_key;
    use super::*;
    use crate::tui::app::ChatLine;
    use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
    use opengoose_board::work_item::RigId;
    use opengoose_rig::rig::Operator;

    fn make_operator(session_id: &str) -> std::sync::Arc<Operator> {
        std::sync::Arc::new(Operator::without_board(
            RigId::new("test"),
            goose::agents::Agent::new(),
            session_id,
        ))
    }

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
        assert_eq!(
            parse_command("/task \"implement feature\""),
            Command::Task("implement feature")
        );
        assert_eq!(parse_command("/task do stuff"), Command::Task("do stuff"));
    }

    #[test]
    fn parse_command_chat() {
        assert_eq!(parse_command("hello world"), Command::Chat("hello world"));
        assert_eq!(parse_command("/unknown"), Command::Chat("/unknown"));
    }

    #[tokio::test]
    async fn handle_key_board_command_refreshes_items_and_pushes_system_line() {
        let mut app = App::new();
        let board = std::sync::Arc::new(
            opengoose_board::Board::in_memory()
                .await
                .expect("in-memory board should initialize"),
        );
        board
            .post(opengoose_board::work_item::PostWorkItem {
                title: "Open item".into(),
                description: String::new(),
                created_by: opengoose_board::work_item::RigId::new("creator"),
                priority: opengoose_board::work_item::Priority::P1,
                tags: vec![],
            })
            .await
            .expect("board operation should succeed");

        let operator = make_operator("s1");
        let (tx, _rx) = mpsc::channel(4);

        app.chat.input = "/board".into();
        let should_quit = handle_key(
            KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE),
            &mut app,
            &tx,
            &board,
            &operator,
        )
        .await;

        assert!(!should_quit);
        assert_eq!(app.board.items.len(), 1);
        assert!(
            app.chat
                .lines
                .iter()
                .any(|line| matches!(line, ChatLine::System(text) if text.starts_with("Board:")))
        );
    }

    #[tokio::test]
    async fn handle_key_task_command_posts_item_without_agent_spawn() {
        let mut app = App::new();
        let board = std::sync::Arc::new(
            opengoose_board::Board::in_memory()
                .await
                .expect("in-memory board should initialize"),
        );
        let operator = make_operator("s2");
        let (tx, _rx) = mpsc::channel(4);

        app.chat.input = "/task \"implement feature\"".into();
        app.agent_busy = true;
        let should_quit = handle_key(
            KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE),
            &mut app,
            &tx,
            &board,
            &operator,
        )
        .await;

        assert!(!should_quit);
        assert!(!app.should_quit);
        assert_eq!(board.list().await.expect("list should succeed").len(), 1);
        assert!(
            app.chat
                .lines
                .iter()
                .any(|line| matches!(line, ChatLine::System(text) if text.contains("posted")))
        );
    }

    #[tokio::test]
    async fn handle_key_invalid_task_usage() {
        let mut app = App::new();
        let board = std::sync::Arc::new(
            opengoose_board::Board::in_memory()
                .await
                .expect("in-memory board should initialize"),
        );
        let operator = make_operator("s3");
        let (tx, _rx) = mpsc::channel(4);

        app.chat.input = "/task".into();
        let should_quit = handle_key(
            KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE),
            &mut app,
            &tx,
            &board,
            &operator,
        )
        .await;

        assert!(!should_quit);
        assert!(app.chat.lines.iter().any(
            |line| matches!(line, ChatLine::System(text) if text == "Usage: /task \"description\"")
        ));
    }

    #[tokio::test]
    async fn handle_key_quit_command_sets_should_quit() {
        let board = std::sync::Arc::new(
            opengoose_board::Board::in_memory()
                .await
                .expect("in-memory board should initialize"),
        );
        let operator = make_operator("quit1");
        let (tx, _rx) = mpsc::channel(4);

        let mut app = App::new();
        app.chat.input = "/quit".into();
        handle_key(
            KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE),
            &mut app,
            &tx,
            &board,
            &operator,
        )
        .await;
        assert!(app.should_quit);

        let mut app2 = App::new();
        app2.chat.input = "/q".into();
        handle_key(
            KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE),
            &mut app2,
            &tx,
            &board,
            &operator,
        )
        .await;
        assert!(app2.should_quit);
    }

    #[tokio::test]
    async fn handle_key_board_refreshes_on_board_command() {
        let mut app = App::new();
        let board = std::sync::Arc::new(
            opengoose_board::Board::in_memory()
                .await
                .expect("in-memory board should initialize"),
        );
        let operator = make_operator("boardcmd1");
        let (tx, _rx) = mpsc::channel(4);

        app.chat.input = "/board".into();
        handle_key(
            KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE),
            &mut app,
            &tx,
            &board,
            &operator,
        )
        .await;
        assert!(
            app.chat
                .lines
                .iter()
                .any(|line| matches!(line, ChatLine::System(text) if text.contains("Board:")))
        );
    }

    #[tokio::test]
    async fn handle_key_task_with_empty_title_shows_usage() {
        let mut app = App::new();
        let board = std::sync::Arc::new(
            opengoose_board::Board::in_memory()
                .await
                .expect("in-memory board should initialize"),
        );
        let operator = make_operator("empty-task1");
        let (tx, _rx) = mpsc::channel(4);

        app.chat.input = "/task \"\"".into();
        handle_key(
            KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE),
            &mut app,
            &tx,
            &board,
            &operator,
        )
        .await;

        assert!(app.chat.lines.iter().any(|line| {
            matches!(line, ChatLine::System(t) if t == "Usage: /task \"description\"")
        }));
    }
}
