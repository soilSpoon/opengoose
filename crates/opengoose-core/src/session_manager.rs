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
    session_store: SessionStore,
    team_store: Option<TeamStore>,
}

impl SessionManager {
    pub fn new(event_bus: EventBus, db: Arc<Database>, team_store: Option<TeamStore>) -> Self {
        // Warm the cache from the database on startup.
        let cache = DashMap::new();
        let session_store = SessionStore::new(db);
        match session_store.load_all_active_teams() {
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
            session_store,
            team_store,
        }
    }

    /// Activate a team for the given session.
    ///
    /// Persists to the database **first**; the in-memory cache is only
    /// updated on success.
    pub fn set_active_team(&self, session_key: &SessionKey, team_name: String) {
        info!(%session_key, team = %team_name, "activating team");

        if let Err(e) = self.session_store.set_active_team(session_key, Some(&team_name)) {
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

        if let Err(e) = self.session_store.set_active_team(session_key, None) {
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
        match self.session_store.get_active_team(session_key) {
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

#[cfg(test)]
mod tests {
    use super::*;

    use opengoose_types::Platform;
    use uuid::Uuid;

    fn test_db() -> Arc<Database> {
        Arc::new(Database::open_in_memory().unwrap())
    }

    fn test_key() -> SessionKey {
        SessionKey::new(Platform::Discord, "guild-1", "channel-1")
    }

    fn temp_team_store() -> TeamStore {
        let dir = std::env::temp_dir().join(format!("opengoose-team-store-{}", Uuid::new_v4()));
        let store = TeamStore::with_dir(dir);
        store.install_defaults(false).unwrap();
        store
    }

    #[test]
    fn restores_cached_teams_from_database() {
        let event_bus = EventBus::new(16);
        let db = test_db();
        let key = test_key();
        SessionStore::new(db.clone())
            .set_active_team(&key, Some("code-review"))
            .unwrap();

        let manager = SessionManager::new(event_bus, db, None);

        assert_eq!(manager.active_team_for(&key), Some("code-review".into()));
    }

    #[test]
    fn set_and_clear_active_team_persist_and_emit_events() {
        let event_bus = EventBus::new(16);
        let mut rx = event_bus.subscribe();
        let db = test_db();
        let key = test_key();
        let manager = SessionManager::new(event_bus, db.clone(), None);

        manager.set_active_team(&key, "code-review".into());
        assert_eq!(manager.active_team_for(&key), Some("code-review".into()));
        assert_eq!(
            SessionStore::new(db.clone()).get_active_team(&key).unwrap(),
            Some("code-review".into())
        );
        assert!(matches!(
            rx.try_recv().unwrap().kind,
            AppEventKind::TeamActivated {
                session_key,
                team_name,
            } if session_key == key && team_name == "code-review"
        ));

        manager.clear_active_team(&key);
        assert_eq!(manager.active_team_for(&key), None);
        assert_eq!(SessionStore::new(db).get_active_team(&key).unwrap(), None);
        assert!(matches!(
            rx.try_recv().unwrap().kind,
            AppEventKind::TeamDeactivated { session_key } if session_key == key
        ));
    }

    #[test]
    fn reads_through_to_database_on_cache_miss() {
        let event_bus = EventBus::new(16);
        let db = test_db();
        let key = test_key();
        let manager = SessionManager::new(event_bus, db.clone(), None);

        SessionStore::new(db)
            .set_active_team(&key, Some("smart-router"))
            .unwrap();

        assert_eq!(manager.active_team_for(&key), Some("smart-router".into()));
    }

    #[test]
    fn team_queries_delegate_to_store() {
        let event_bus = EventBus::new(16);
        let db = test_db();
        let manager = SessionManager::new(event_bus, db, Some(temp_team_store()));

        assert!(manager.team_exists("code-review"));
        assert!(!manager.team_exists("missing-team"));
        assert_eq!(
            manager.list_teams(),
            vec![
                "code-review".to_string(),
                "research-panel".to_string(),
                "smart-router".to_string()
            ]
        );
        assert!(manager.team_store().is_some());
    }

    #[test]
    fn missing_team_store_returns_safe_defaults() {
        let event_bus = EventBus::new(16);
        let db = test_db();
        let manager = SessionManager::new(event_bus, db, None);

        assert!(!manager.team_exists("code-review"));
        assert!(manager.list_teams().is_empty());
        assert!(manager.team_store().is_none());
    }
}
