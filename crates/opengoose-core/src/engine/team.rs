use opengoose_types::SessionKey;

use super::Engine;

impl Engine {
    // ── Team management ─────────────────────────────────────────────

    pub fn set_active_team(&self, session_key: &SessionKey, team_name: String) {
        self.session_manager.set_active_team(session_key, team_name);
    }

    pub fn clear_active_team(&self, session_key: &SessionKey) {
        self.session_manager.clear_active_team(session_key);
    }

    pub fn active_team_for(&self, session_key: &SessionKey) -> Option<String> {
        self.session_manager.active_team_for(session_key)
    }

    pub fn team_exists(&self, name: &str) -> bool {
        self.session_manager.team_exists(name)
    }

    pub fn list_teams(&self) -> Vec<String> {
        self.session_manager.list_teams()
    }

    // ── Team command handling ─────────────────────────────────────────

    /// Handle a `/team` command and return the response text.
    ///
    /// Centralises team activation/deactivation/listing logic that was
    /// previously duplicated across every channel gateway.
    pub fn handle_team_command(&self, session_key: &SessionKey, args: &str) -> String {
        match args {
            "" => match self.active_team_for(session_key) {
                Some(team) => format!("Active team: {team}"),
                None => "No team active for this channel.".to_string(),
            },
            "off" => {
                self.clear_active_team(session_key);
                "Team deactivated. Reverting to single-agent mode.".to_string()
            }
            "list" => {
                let teams = self.list_teams();
                if teams.is_empty() {
                    "No teams available.".to_string()
                } else {
                    format!(
                        "Available teams:\n{}",
                        teams
                            .iter()
                            .map(|t| format!("- {t}"))
                            .collect::<Vec<_>>()
                            .join("\n")
                    )
                }
            }
            team_name => {
                if self.team_exists(team_name) {
                    self.set_active_team(session_key, team_name.to_string());
                    format!("Team {team_name} activated for this channel.")
                } else {
                    let available = self.list_teams();
                    format!(
                        "Team `{team_name}` not found. Available: {}",
                        if available.is_empty() {
                            "none".to_string()
                        } else {
                            available.join(", ")
                        }
                    )
                }
            }
        }
    }
}
