use std::sync::Arc;

use opengoose_persistence::{Database, OrchestrationStore, SessionStore};
use opengoose_profiles::ProfileStore;
use opengoose_teams::TeamStore;

/// Shared application state passed to all handlers.
#[derive(Clone)]
pub struct AppState {
    pub session_store: Arc<SessionStore>,
    pub orchestration_store: Arc<OrchestrationStore>,
    pub profile_store: Arc<ProfileStore>,
    pub team_store: Arc<TeamStore>,
}

impl AppState {
    /// Create AppState from an existing shared Database.
    pub fn new(db: Arc<Database>) -> anyhow::Result<Self> {
        Ok(Self {
            session_store: Arc::new(SessionStore::new(db.clone())),
            orchestration_store: Arc::new(OrchestrationStore::new(db)),
            profile_store: Arc::new(ProfileStore::new()?),
            team_store: Arc::new(TeamStore::new()?),
        })
    }
}
