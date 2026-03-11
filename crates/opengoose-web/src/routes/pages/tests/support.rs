use std::future::Future;
use std::sync::Arc;

use axum::body::to_bytes;
use opengoose_persistence::{ApiKeyStore, Database, OrchestrationStore, SessionStore};
use opengoose_teams::remote::{RemoteAgentRegistry, RemoteConfig};
use opengoose_teams::{OrchestrationPattern, TeamAgent, TeamDefinition, TeamStore};
use opengoose_types::{ChannelMetricsStore, EventBus, SessionKey};

use crate::server::PageState;

pub(super) const TEMP_HOME_PREFIX: &str = "opengoose-routes-pages-home";

pub(super) fn test_db() -> Arc<Database> {
    Arc::new(Database::open_in_memory().expect("db should open"))
}

pub(super) fn save_team(name: &str) {
    TeamStore::new()
        .expect("team store should open")
        .save(
            &TeamDefinition {
                version: "1.0.0".into(),
                title: name.into(),
                description: Some(format!("{name} team")),
                workflow: OrchestrationPattern::Chain,
                agents: vec![TeamAgent {
                    profile: "tester".into(),
                    role: Some("validate setup".into()),
                }],
                router: None,
                fan_out: None,
                goal: None,
            },
            true,
        )
        .expect("team should save");
}

pub(super) fn page_state(db: Arc<Database>) -> PageState {
    PageState {
        api_key_store: Arc::new(ApiKeyStore::new(db.clone())),
        db,
        remote_registry: RemoteAgentRegistry::new(RemoteConfig::default()),
        channel_metrics: ChannelMetricsStore::new(),
        event_bus: EventBus::new(256),
    }
}

pub(super) fn run_async(test: impl Future<Output = ()>) {
    tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .expect("runtime should build")
        .block_on(test);
}

pub(super) fn save_session(db: Arc<Database>, key: &SessionKey, active_team: Option<&str>) {
    let store = SessionStore::new(db);
    store
        .append_user_message(key, "Need a reviewer on this run.", Some("tester"))
        .expect("session should seed");
    if let Some(team) = active_team {
        store
            .set_active_team(key, Some(team))
            .expect("active team should seed");
    }
}

pub(super) fn save_run(db: Arc<Database>, run_id: &str) {
    OrchestrationStore::new(db)
        .create_run(
            run_id,
            "discord:ns:ops:chan-1",
            "ops",
            "chain",
            "Review the latest deploy.",
            3,
        )
        .expect("run should seed");
}

pub(super) async fn read_body(response: axum::response::Response) -> String {
    let body = to_bytes(response.into_body(), 1024 * 1024)
        .await
        .expect("response body should be readable");
    String::from_utf8(body.to_vec()).expect("response body should be utf-8")
}
