use std::sync::Arc;

use dashmap::DashMap;
use tracing::{info, warn};

use opengoose_persistence::{Database, SessionStore};
use opengoose_teams::TeamStore;
use opengoose_types::{AppEventKind, EventBus, SessionKey};

/// Manages per-session active team state.
///
/// The database is the single source of truth. An in-memory cache
/// (`DashMap`) is kept in sync but is never trusted over the DB:
///
/// - **Writes** go to the DB first; only on success is the cache updated.
/// - **Reads** hit the cache first; on a miss the DB is consulted and
///   the cache is populated (read-through).
///
/// This eliminates the previous consistency gap where the `DashMap` was
/// updated optimistically before the DB write, leaving stale entries if
/// the write failed.
pub struct SessionManager {
    event_bus: EventBus,
    /// In-memory cache: SessionKey -> team name.
    cache: DashMap<SessionKey, String>,
    db: Arc<Database>,
    team_store: Option<TeamStore>,
}

impl SessionManager {
    pub fn new(event_bus: EventBus, db: Arc<Database>, team_store: Option<TeamStore>) -> Self {
        // Warm the cache from the database on startup.
        let cache = DashMap::new();
        let sessions = SessionStore::new(db.clone());
        match sessions.load_all_active_teams() {
            Ok(teams) => {
                for (key, team) in teams {
                    info!(%key, team = %team, "restored active team from db");
                    cache.insert(key, team);
                }
            }
            Err(e) => {
                warn!(%e, "failed to restore active teams from db");
            }
        }

        Self {
            event_bus,
            cache,
            db,
            team_store,
        }
    }

    /// Activate a team for the given session.
    ///
    /// Persists to the database **first**; the in-memory cache is only
    /// updated on success.
    pub fn set_active_team(&self, session_key: &SessionKey, team_name: String) {
        info!(%session_key, team = %team_name, "activating team");

        let sessions = SessionStore::new(self.db.clone());
        if let Err(e) = sessions.set_active_team(session_key, Some(&team_name)) {
            warn!(%e, "failed to persist active team — cache NOT updated");
            return;
        }

        // DB succeeded — now update the cache.
        self.cache.insert(session_key.clone(), team_name.clone());

        self.event_bus.emit(AppEventKind::TeamActivated {
            session_key: session_key.clone(),
            team_name,
        });
    }

    /// Deactivate the team for the given session.
    ///
    /// Persists to the database **first**; the in-memory cache is only
    /// cleared on success.
    pub fn clear_active_team(&self, session_key: &SessionKey) {
        info!(%session_key, "deactivating team");

        let sessions = SessionStore::new(self.db.clone());
        if let Err(e) = sessions.set_active_team(session_key, None) {
            warn!(%e, "failed to persist team deactivation — cache NOT updated");
            return;
        }

        // DB succeeded — now remove from cache.
        self.cache.remove(session_key);

        self.event_bus.emit(AppEventKind::TeamDeactivated {
            session_key: session_key.clone(),
        });
    }

    /// Look up the active team for a session.
    ///
    /// Checks the in-memory cache first. On a miss, falls back to the
    /// database and populates the cache (read-through).
    pub fn active_team_for(&self, session_key: &SessionKey) -> Option<String> {
        // Cache hit
        if let Some(entry) = self.cache.get(session_key) {
            return Some(entry.clone());
        }

        // Cache miss — read-through from DB
        let sessions = SessionStore::new(self.db.clone());
        match sessions.get_active_team(session_key) {
            Ok(Some(team)) => {
                self.cache.insert(session_key.clone(), team.clone());
                Some(team)
            }
            Ok(None) => None,
            Err(e) => {
                warn!(%e, "failed to read active team from db");
                None
            }
        }
    }

    pub fn team_exists(&self, name: &str) -> bool {
        match &self.team_store {
            Some(store) => store.get(name).is_ok(),
            None => false,
        }
    }

    pub fn list_teams(&self) -> Vec<String> {
        match &self.team_store {
            Some(store) => store.list().unwrap_or_default(),
            None => Default::default(),
        }
    }

    pub fn team_store(&self) -> Option<&TeamStore> {
        self.team_store.as_ref()
    }
}
