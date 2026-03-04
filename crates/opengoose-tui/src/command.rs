use nucleo_matcher::pattern::{AtomKind, CaseMatching, Normalization, Pattern};
use nucleo_matcher::{Config, Matcher};

use crate::app::App;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CommandId {
    SetDiscordToken,
    GeneratePairingCode,
    ClearMessages,
    ClearEvents,
    Quit,
}

#[derive(Debug, Clone)]
pub struct Command {
    pub id: CommandId,
    pub label: &'static str,
    pub score: Option<u32>,
}

pub fn get_commands() -> Vec<Command> {
    vec![
        Command { id: CommandId::SetDiscordToken, label: "Set Discord Token", score: None },
        Command { id: CommandId::GeneratePairingCode, label: "Generate Pairing Code", score: None },
        Command { id: CommandId::ClearMessages, label: "Clear Messages", score: None },
        Command { id: CommandId::ClearEvents, label: "Clear Events", score: None },
        Command { id: CommandId::Quit, label: "Quit", score: None },
    ]
}

pub fn filter_commands<'a>(commands: &'a [Command], query: &str) -> Vec<Command> {
    if query.is_empty() {
        return commands.to_vec();
    }

    let mut matcher = Matcher::new(Config::DEFAULT.match_paths());
    let pattern = Pattern::new(query, CaseMatching::Ignore, Normalization::Smart, AtomKind::Fuzzy);

    let mut scored: Vec<Command> = commands
        .iter()
        .filter_map(|cmd| {
            let mut buf = Vec::new();
            let haystack = nucleo_matcher::Utf32Str::new(cmd.label, &mut buf);
            let score = pattern.score(haystack, &mut matcher)?;
            Some(Command {
                id: cmd.id,
                label: cmd.label,
                score: Some(score),
            })
        })
        .collect();

    scored.sort_by(|a, b| b.score.cmp(&a.score));
    scored
}

pub fn execute(app: &mut App, id: CommandId) {
    match id {
        CommandId::SetDiscordToken => {
            app.secret_input.visible = true;
            app.secret_input.input.clear();
            app.secret_input.status_message = None;
        }
        CommandId::GeneratePairingCode => {
            if let Some(tx) = &app.pairing_tx {
                let _ = tx.send(());
            }
        }
        CommandId::ClearMessages => app.clear_messages(),
        CommandId::ClearEvents => app.clear_events(),
        CommandId::Quit => app.should_quit = true,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_get_commands_count() {
        assert_eq!(get_commands().len(), 5);
    }

    #[test]
    fn test_filter_commands_empty_query() {
        let commands = get_commands();
        let filtered = filter_commands(&commands, "");
        assert_eq!(filtered.len(), 5);
    }

    #[test]
    fn test_filter_commands_fuzzy_match() {
        let commands = get_commands();
        let filtered = filter_commands(&commands, "quit");
        assert!(filtered.iter().any(|c| c.id == CommandId::Quit));

        let filtered = filter_commands(&commands, "zzzzz");
        assert!(filtered.is_empty());
    }
}
