use thiserror::Error;

#[derive(Debug, Error)]
pub enum WorkflowError {
    #[error("workflow not found: {name}")]
    NotFound { name: String },

    #[error("invalid workflow definition: {reason}")]
    InvalidDefinition { reason: String },

    #[error("step '{step}' references unknown agent '{agent}'")]
    UnknownAgent { step: String, agent: String },

    #[error("step '{step}' depends on unknown step '{dependency}'")]
    UnknownDependency { step: String, dependency: String },

    #[error("circular dependency detected: {chain}")]
    CyclicDependency { chain: String },

    #[error("step '{step}': dependency '{dependency}' has no output (status: {status})")]
    UnsatisfiedDependency {
        step: String,
        dependency: String,
        status: String,
    },

    #[error("step '{step}' failed after {retries} retries")]
    StepFailed { step: String, retries: u32 },

    #[error("workflow '{workflow}' is already running")]
    AlreadyRunning { workflow: String },

    #[error(transparent)]
    Yaml(#[from] serde_yaml::Error),

    #[error(transparent)]
    Io(#[from] std::io::Error),
}
