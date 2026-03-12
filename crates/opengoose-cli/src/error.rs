use thiserror::Error;

#[derive(Error, Debug)]
pub enum CliError {
    #[error("persistence: {0}")]
    Persistence(#[from] opengoose_persistence::PersistenceError),

    #[error("team: {0}")]
    Team(#[from] opengoose_teams::TeamError),

    #[error("project: {0}")]
    Project(#[from] opengoose_projects::ProjectError),

    #[error("profile: {0}")]
    Profile(#[from] opengoose_profiles::ProfileError),

    #[error("gateway: {0}")]
    Gateway(#[from] opengoose_core::GatewayError),

    #[error("io: {0}")]
    Io(#[from] std::io::Error),

    #[error("http: {0}")]
    Reqwest(#[from] reqwest::Error),

    #[error("websocket: {0}")]
    WebSocket(#[from] tokio_tungstenite::tungstenite::Error),

    #[error("task join: {0}")]
    Join(#[from] tokio::task::JoinError),

    #[error("json: {0}")]
    SerdeJson(#[from] serde_json::Error),

    #[error("secrets: {0}")]
    Secrets(#[from] opengoose_secrets::SecretError),

    #[error("{0}")]
    Validation(String),

    #[error("{0}")]
    Other(#[from] anyhow::Error),
}

pub type CliResult<T> = Result<T, CliError>;
