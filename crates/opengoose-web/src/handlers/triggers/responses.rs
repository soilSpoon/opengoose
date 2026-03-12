use serde::Serialize;

use opengoose_persistence::Trigger;

/// JSON representation of a trigger.
#[derive(Debug, Serialize)]
pub struct TriggerResponse {
    pub name: String,
    pub trigger_type: String,
    pub condition_json: String,
    pub team_name: String,
    pub input: String,
    pub enabled: bool,
    pub fire_count: i32,
    pub last_fired_at: Option<String>,
    pub created_at: String,
    pub updated_at: String,
}

impl From<Trigger> for TriggerResponse {
    fn from(trigger: Trigger) -> Self {
        Self {
            name: trigger.name,
            trigger_type: trigger.trigger_type,
            condition_json: trigger.condition_json,
            team_name: trigger.team_name,
            input: trigger.input,
            enabled: trigger.enabled,
            fire_count: trigger.fire_count,
            last_fired_at: trigger.last_fired_at,
            created_at: trigger.created_at,
            updated_at: trigger.updated_at,
        }
    }
}
