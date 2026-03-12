use nucleo_matcher::pattern::{AtomKind, CaseMatching, Normalization, Pattern};
use nucleo_matcher::{Config, Matcher};

use crate::app::{App, ProviderSelectPurpose};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CommandId {
    ConfigureProvider,
    ListModels,
    SetDiscordToken,
    GeneratePairingCode,
    ListSessions,
    ListTeams,
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
        Command {
            id: CommandId::ConfigureProvider,
            label: "Configure AI Provider",
            score: None,
        },
        Command {
            id: CommandId::ListModels,
            label: "List Provider Models",
            score: None,
        },
        Command {
            id: CommandId::SetDiscordToken,
            label: "Set Discord Token",
            score: None,
        },
        Command {
            id: CommandId::GeneratePairingCode,
            label: "Generate Pairing Code",
            score: None,
        },
        Command {
            id: CommandId::ListSessions,
            label: "Open Session Browser",
            score: None,
        },
        Command {
            id: CommandId::ListTeams,
            label: "List Available Teams",
            score: None,
        },
        Command {
            id: CommandId::ClearMessages,
            label: "Clear Messages",
            score: None,
        },
        Command {
            id: CommandId::ClearEvents,
            label: "Clear Events",
            score: None,
        },
        Command {
            id: CommandId::Quit,
            label: "Quit",
            score: None,
        },
    ]
}

pub fn filter_commands(commands: &[Command], query: &str) -> Vec<Command> {
    if query.is_empty() {
        return commands.to_vec();
    }

    let mut matcher = Matcher::new(Config::DEFAULT.match_paths());
    let pattern = Pattern::new(
        query,
        CaseMatching::Ignore,
        Normalization::Smart,
        AtomKind::Fuzzy,
    );

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

    scored.sort_by_key(|cmd| std::cmp::Reverse(cmd.score));
    scored
}

pub fn execute(app: &mut App, id: CommandId) {
    match id {
        CommandId::ConfigureProvider => {
            app.open_provider_select();
        }
        CommandId::ListModels => {
            app.open_provider_select_for(ProviderSelectPurpose::ListModels);
        }
        CommandId::SetDiscordToken => {
            app.secret_input.visible = true;
            app.secret_input.input.clear();
            app.secret_input.status_message = None;
            app.secret_input.title = None;
            app.secret_input.is_secret = true;
        }
        CommandId::GeneratePairingCode => {
            if let Some(tx) = &app.pairing_tx {
                let _ = tx.send(());
            }
        }
        CommandId::ListSessions => {
            if app.sessions.is_empty() {
                app.push_event("No sessions available yet.", crate::app::EventLevel::Info);
            } else {
                app.focus_sessions();
            }
        }
        CommandId::ListTeams => match opengoose_teams::TeamStore::new() {
            Ok(store) => match store.list() {
                Ok(teams) => {
                    let msg = if teams.is_empty() {
                        "No teams found. Run `opengoose team init` to install defaults.".to_string()
                    } else {
                        format!("Available teams: {}", teams.join(", "))
                    };
                    app.push_event(&msg, crate::app::EventLevel::Info);
                }
                Err(e) => {
                    app.push_event(
                        &format!("Failed to list teams: {e}"),
                        crate::app::EventLevel::Error,
                    );
                }
            },
            Err(e) => {
                app.push_event(
                    &format!("Failed to open team store: {e}"),
                    crate::app::EventLevel::Error,
                );
            }
        },
        CommandId::ClearMessages => app.clear_messages(),
        CommandId::ClearEvents => app.clear_events(),
        CommandId::Quit => app.should_quit = true,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::app::AppMode;

    fn test_app() -> App {
        App::new(AppMode::Normal, None, None)
    }

    #[test]
    fn test_get_commands_count() {
        assert_eq!(get_commands().len(), 9);
    }

    #[test]
    fn test_filter_commands_empty_query() {
        let commands = get_commands();
        let filtered = filter_commands(&commands, "");
        assert_eq!(filtered.len(), 9);
    }

    #[test]
    fn test_filter_commands_fuzzy_match() {
        let commands = get_commands();
        let filtered = filter_commands(&commands, "quit");
        assert!(filtered.iter().any(|c| c.id == CommandId::Quit));

        let filtered = filter_commands(&commands, "zzzzz");
        assert!(filtered.is_empty());
    }

    #[test]
    fn test_filter_commands_partial_match() {
        let commands = get_commands();
        let filtered = filter_commands(&commands, "token");
        assert!(filtered.iter().any(|c| c.id == CommandId::SetDiscordToken));
    }

    #[test]
    fn test_filter_commands_scores_present() {
        let commands = get_commands();
        let filtered = filter_commands(&commands, "clear");
        for cmd in &filtered {
            assert!(cmd.score.is_some());
        }
    }

    #[test]
    fn test_execute_quit() {
        let mut app = test_app();
        execute(&mut app, CommandId::Quit);
        assert!(app.should_quit);
    }

    #[test]
    fn test_execute_set_discord_token() {
        let mut app = test_app();
        execute(&mut app, CommandId::SetDiscordToken);
        assert!(app.secret_input.visible);
        assert!(app.secret_input.input.is_empty());
        assert!(app.secret_input.status_message.is_none());
    }

    #[test]
    fn test_execute_clear_messages() {
        let mut app = test_app();
        app.messages.push_back(crate::app::MessageEntry {
            session_key: opengoose_types::SessionKey::dm(opengoose_types::Platform::Discord, "u"),
            author: "a".into(),
            content: "c".into(),
        });
        execute(&mut app, CommandId::ClearMessages);
        assert!(app.messages.is_empty());
    }

    #[test]
    fn test_execute_clear_events() {
        let mut app = test_app();
        app.push_event("test", crate::app::EventLevel::Info);
        execute(&mut app, CommandId::ClearEvents);
        assert!(app.events.is_empty());
    }

    #[test]
    fn test_execute_generate_pairing_code_no_tx() {
        let mut app = test_app();
        // Should not panic when pairing_tx is None
        execute(&mut app, CommandId::GeneratePairingCode);
    }

    #[test]
    fn test_execute_generate_pairing_code_with_tx() {
        let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel();
        let mut app = App::new(AppMode::Normal, None, Some(tx));
        execute(&mut app, CommandId::GeneratePairingCode);
        assert!(rx.try_recv().is_ok());
    }

    #[test]
    fn test_execute_list_sessions_empty() {
        let mut app = test_app();
        execute(&mut app, CommandId::ListSessions);
        assert_eq!(app.events.len(), 1);
        assert!(
            app.events
                .back()
                .unwrap()
                .summary
                .contains("No sessions available yet")
        );
    }

    #[test]
    fn test_execute_list_sessions_with_sessions() {
        let mut app = test_app();
        let sk = opengoose_types::SessionKey::dm(opengoose_types::Platform::Discord, "user1");
        app.sessions.push(crate::app::SessionListEntry {
            session_key: sk,
            active_team: None,
            created_at: None,
            updated_at: None,
            is_active: true,
        });
        execute(&mut app, CommandId::ListSessions);
        assert_eq!(app.active_panel, crate::app::Panel::Sessions);
    }
}
