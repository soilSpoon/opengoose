use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};

use opengoose_persistence::{
    AlertStore, ApiKeyStore, Database, OrchestrationStore, ScheduleStore, SessionStore,
    TriggerStore,
};
use opengoose_profiles::{AgentProfile, ProfileStore};
use opengoose_teams::{
    CommunicationMode, OrchestrationPattern, TeamAgent, TeamDefinition, TeamStore,
};
use opengoose_types::{ChannelMetricsStore, EventBus};

use crate::state::AppState;

static UNIQUE_COUNTER: AtomicU64 = AtomicU64::new(0);

pub(crate) fn unique_temp_path(label: &str) -> PathBuf {
    let suffix = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("system time should be after epoch")
        .as_nanos();
    let counter = UNIQUE_COUNTER.fetch_add(1, Ordering::Relaxed);
    std::env::temp_dir().join(format!(
        "opengoose-web-{label}-{}-{suffix}-{counter}",
        std::process::id(),
    ))
}

pub(crate) fn unique_temp_dir(label: &str) -> PathBuf {
    let dir = unique_temp_path(label);
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).expect("temp test dir should be created");
    dir
}

pub(crate) fn make_state() -> AppState {
    make_state_with_dirs(unique_temp_dir("profiles"), unique_temp_dir("teams"))
}

pub(crate) fn make_state_with_dirs(profile_dir: PathBuf, team_dir: PathBuf) -> AppState {
    let db = Arc::new(Database::open_in_memory().expect("in-memory db should open"));
    AppState {
        db: db.clone(),
        session_store: Arc::new(SessionStore::new(db.clone())),
        orchestration_store: Arc::new(OrchestrationStore::new(db.clone())),
        profile_store: Arc::new(ProfileStore::with_dir(profile_dir)),
        team_store: Arc::new(TeamStore::with_dir(team_dir)),
        schedule_store: Arc::new(ScheduleStore::new(db.clone())),
        trigger_store: Arc::new(TriggerStore::new(db.clone())),
        alert_store: Arc::new(AlertStore::new(db.clone())),
        api_key_store: Arc::new(ApiKeyStore::new(db)),
        channel_metrics: ChannelMetricsStore::new(),
        event_bus: EventBus::new(256),
    }
}

pub(crate) fn sample_profile(title: &str) -> AgentProfile {
    AgentProfile {
        version: "1.0".into(),
        title: title.into(),
        description: Some(format!("{title} profile")),
        instructions: None,
        prompt: None,
        extensions: vec![],
        skills: vec![],
        settings: None,
        activities: None,
        response: None,
        sub_recipes: None,
        parameters: None,
    }
}

pub(crate) fn sample_team(title: &str, profile: &str) -> TeamDefinition {
    TeamDefinition {
        version: "1.0".into(),
        title: title.into(),
        description: Some(format!("{title} team")),
        workflow: OrchestrationPattern::Chain,
        communication_mode: CommunicationMode::default(),
        agents: vec![TeamAgent {
            profile: profile.into(),
            role: Some("test agent".into()),
        }],
        router: None,
        fan_out: None,
        goal: None,
    }
}
