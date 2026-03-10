use std::collections::{HashMap, VecDeque};
use std::sync::Arc;

use opengoose_persistence::SessionStore;
use opengoose_types::SessionKey;

use super::session_types::{EventLevel, MAX_MESSAGES, MessageEntry, Panel, SessionListEntry};
use super::state::App;
const SESSION_HISTORY_LIMIT: usize = 200;
const SESSION_LIST_LIMIT: i64 = 100;

impl App {
    pub fn attach_session_store(&mut self, session_store: Arc<SessionStore>) {
        self.session_store = Some(session_store.clone());

        if let Ok(active_teams) = session_store.load_all_active_teams() {
            self.active_teams.extend(active_teams);
        }

        self.refresh_sessions();
    }

    pub fn refresh_sessions(&mut self) {
        let mut session_map: HashMap<String, SessionListEntry> = self
            .sessions
            .iter()
            .cloned()
            .map(|entry| (entry.session_key.to_stable_id(), entry))
            .collect();

        if let Some(store) = &self.session_store {
            match store.list_sessions(SESSION_LIST_LIMIT) {
                Ok(items) => {
                    for item in items {
                        let session_key = SessionKey::from_stable_id(&item.session_key);
                        session_map.insert(
                            item.session_key,
                            SessionListEntry {
                                active_team: item
                                    .active_team
                                    .or_else(|| self.active_teams.get(&session_key).cloned()),
                                created_at: Some(item.created_at),
                                updated_at: Some(item.updated_at),
                                is_active: self.active_sessions.contains(&session_key),
                                session_key,
                            },
                        );
                    }
                }
                Err(e) => {
                    let message = format!("Could not load session history: {e}");
                    self.push_event(&message, EventLevel::Error);
                    self.set_status_notice(message, EventLevel::Error);
                }
            }
        }

        for session_key in &self.active_sessions {
            let stable = session_key.to_stable_id();
            session_map
                .entry(stable)
                .and_modify(|entry| {
                    entry.is_active = true;
                    if entry.active_team.is_none() {
                        entry.active_team = self.active_teams.get(session_key).cloned();
                    }
                })
                .or_insert_with(|| SessionListEntry {
                    session_key: session_key.clone(),
                    active_team: self.active_teams.get(session_key).cloned(),
                    created_at: None,
                    updated_at: None,
                    is_active: true,
                });
        }

        let previous_selection = self.selected_session.clone();
        let mut sessions = session_map.into_values().collect::<Vec<_>>();
        sessions.sort_by(|left, right| {
            right
                .is_active
                .cmp(&left.is_active)
                .then_with(|| right.updated_at.cmp(&left.updated_at))
                .then_with(|| right.created_at.cmp(&left.created_at))
                .then_with(|| {
                    left.session_key
                        .to_stable_id()
                        .cmp(&right.session_key.to_stable_id())
                })
        });
        self.sessions = sessions;

        if self.sessions.is_empty() {
            self.selected_session = None;
            self.selected_session_index = 0;
            self.messages.clear();
            self.messages_scroll = 0;
            return;
        }

        if let Some(session_key) = previous_selection
            && let Some(index) = self
                .sessions
                .iter()
                .position(|entry| entry.session_key == session_key)
        {
            self.selected_session_index = index;
            self.selected_session = Some(self.sessions[index].session_key.clone());
            self.ensure_selected_session_visible();
            if self.messages.is_empty() {
                self.load_selected_session_history();
            }
            return;
        }

        self.select_session(self.selected_session_index.min(self.sessions.len() - 1));
    }

    pub fn clear_messages(&mut self) {
        if let Some(session_key) = &self.selected_session {
            self.session_messages.remove(session_key);
        } else {
            self.session_messages.clear();
        }
        self.messages.clear();
        self.messages_scroll = 0;
    }

    pub fn focus_sessions(&mut self) {
        self.active_panel = Panel::Sessions;
        if self.selected_session.is_none() && !self.sessions.is_empty() {
            self.select_session(0);
        }
    }

    pub fn select_next_session(&mut self) {
        if self.sessions.is_empty() {
            return;
        }
        let next = (self.selected_session_index + 1).min(self.sessions.len() - 1);
        self.select_session(next);
    }

    pub fn select_previous_session(&mut self) {
        if self.sessions.is_empty() {
            return;
        }
        self.select_session(self.selected_session_index.saturating_sub(1));
    }

    pub fn select_first_session(&mut self) {
        if !self.sessions.is_empty() {
            self.select_session(0);
        }
    }

    pub fn select_last_session(&mut self) {
        if !self.sessions.is_empty() {
            self.select_session(self.sessions.len() - 1);
        }
    }

    pub fn select_session(&mut self, index: usize) {
        if self.sessions.is_empty() {
            return;
        }

        self.selected_session_index = index.min(self.sessions.len() - 1);
        self.selected_session = Some(
            self.sessions[self.selected_session_index]
                .session_key
                .clone(),
        );
        self.ensure_selected_session_visible();
        self.load_selected_session_history();
        self.messages_scroll = 0;
    }

    pub fn cache_message(&mut self, entry: MessageEntry) {
        let session_key = entry.session_key.clone();
        if let Some(index) = self
            .sessions
            .iter()
            .position(|session| session.session_key == session_key)
        {
            let mut session = self.sessions.remove(index);
            session.is_active = self.active_sessions.contains(&session_key) || session.is_active;
            session.active_team = self
                .active_teams
                .get(&session_key)
                .cloned()
                .or(session.active_team);
            self.sessions.insert(0, session);
            if self.selected_session.as_ref() == Some(&session_key) {
                self.selected_session_index = 0;
            }
        } else {
            self.sessions.insert(
                0,
                SessionListEntry {
                    session_key: session_key.clone(),
                    active_team: self.active_teams.get(&session_key).cloned(),
                    created_at: None,
                    updated_at: None,
                    is_active: self.active_sessions.contains(&session_key),
                },
            );
        }

        let messages = self
            .session_messages
            .entry(session_key.clone())
            .or_default();
        messages.push_back(entry);
        if messages.len() > MAX_MESSAGES {
            messages.pop_front();
        }

        if self.selected_session.as_ref() == Some(&session_key) {
            self.messages = messages.clone();
            self.messages_scroll = 0;
        } else if self.selected_session.is_none() {
            self.selected_session_index = 0;
            self.selected_session = Some(session_key);
            self.load_selected_session_history();
            self.messages_scroll = 0;
        }
    }

    fn ensure_selected_session_visible(&mut self) {
        if self.sessions_area_height == 0 {
            return;
        }

        if self.selected_session_index < self.sessions_scroll {
            self.sessions_scroll = self.selected_session_index;
            return;
        }

        let last_visible = self
            .sessions_scroll
            .saturating_add(self.sessions_area_height.saturating_sub(1));
        if self.selected_session_index > last_visible {
            self.sessions_scroll = self
                .selected_session_index
                .saturating_add(1)
                .saturating_sub(self.sessions_area_height);
        }
    }

    fn load_selected_session_history(&mut self) {
        let Some(session_key) = self.selected_session.clone() else {
            self.messages.clear();
            return;
        };

        if !self.session_messages.contains_key(&session_key)
            && let Some(store) = &self.session_store
        {
            match store.load_history(&session_key, SESSION_HISTORY_LIMIT) {
                Ok(history) => {
                    let messages = history
                        .into_iter()
                        .map(|message| MessageEntry {
                            session_key: session_key.clone(),
                            author: message
                                .author
                                .unwrap_or_else(|| match message.role.as_str() {
                                    "assistant" => "goose".to_string(),
                                    _ => "user".to_string(),
                                }),
                            content: message.content,
                        })
                        .collect::<VecDeque<_>>();
                    self.session_messages.insert(session_key.clone(), messages);
                }
                Err(e) => {
                    let message = format!(
                        "Could not load history for {}: {e}",
                        Self::format_session_label(&session_key)
                    );
                    self.push_event(&message, EventLevel::Error);
                    self.set_status_notice(message, EventLevel::Error);
                }
            }
        }

        self.messages = self
            .session_messages
            .get(&session_key)
            .cloned()
            .unwrap_or_default();
    }
}
