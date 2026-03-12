use serde::Deserialize;

/// JSON body for creating a new trigger.
#[derive(Deserialize)]
pub struct CreateTriggerRequest {
    pub name: String,
    /// One of: webhook_received, file_watch, cron, message_received
    pub trigger_type: String,
    /// JSON object with type-specific condition fields.
    pub condition_json: Option<String>,
    pub team_name: String,
    pub input: Option<String>,
}

/// JSON body for updating an existing trigger.
#[derive(Deserialize)]
pub struct UpdateTriggerRequest {
    pub trigger_type: String,
    pub condition_json: Option<String>,
    pub team_name: String,
    pub input: Option<String>,
}

/// JSON body for enabling or disabling a trigger.
#[derive(Deserialize)]
pub struct SetEnabledRequest {
    pub enabled: bool,
}

/// JSON body for firing a test event.
#[derive(Default, Deserialize)]
pub struct TestTriggerRequest {
    pub input: Option<String>,
}
