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

    #[error("{0}")]
    Other(#[from] anyhow::Error),
}

pub type CliResult<T> = Result<T, CliError>;
