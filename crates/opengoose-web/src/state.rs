use std::sync::Arc;

use opengoose_persistence::{AlertStore, Database, OrchestrationStore, SessionStore};
use opengoose_profiles::ProfileStore;
use opengoose_teams::TeamStore;

/// Shared application state passed to all handlers.
#[derive(Clone)]
pub struct AppState {
    /// Underlying SQLite database handle.
    pub db: Arc<Database>,
    /// Store for chat sessions and message history.
    pub session_store: Arc<SessionStore>,
    /// Store for team orchestration runs.
    pub orchestration_store: Arc<OrchestrationStore>,
    /// Store for agent profile YAML definitions.
    pub profile_store: Arc<ProfileStore>,
    /// Store for team YAML definitions.
    pub team_store: Arc<TeamStore>,
    /// Store for monitoring alert rules and history.
    pub alert_store: Arc<AlertStore>,
}

impl AppState {
    /// Create AppState from an existing shared Database.
    pub fn new(db: Arc<Database>) -> anyhow::Result<Self> {
        Ok(Self {
            session_store: Arc::new(SessionStore::new(db.clone())),
            orchestration_store: Arc::new(OrchestrationStore::new(db.clone())),
            alert_store: Arc::new(AlertStore::new(db.clone())),
            profile_store: Arc::new(ProfileStore::new()?),
            team_store: Arc::new(TeamStore::new()?),
            db,
        })
    }
}
