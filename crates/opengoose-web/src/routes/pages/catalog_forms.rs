use serde::Deserialize;

#[derive(Deserialize, Default)]
pub(crate) struct SessionQuery {
    pub(crate) session: Option<String>,
}

#[derive(Deserialize, Default)]
pub(crate) struct RunQuery {
    pub(crate) run: Option<String>,
}

#[derive(Deserialize, Default)]
pub(crate) struct AgentQuery {
    pub(crate) agent: Option<String>,
}

#[derive(Deserialize, Default)]
pub(crate) struct TeamQuery {
    pub(crate) team: Option<String>,
}

#[derive(Deserialize, Default)]
pub(crate) struct WorkflowQuery {
    pub(crate) workflow: Option<String>,
}

#[derive(Deserialize, Default)]
pub(crate) struct ScheduleQuery {
    pub(crate) schedule: Option<String>,
}

#[derive(Deserialize, Default)]
pub(crate) struct TriggerQuery {
    pub(crate) trigger: Option<String>,
}

#[derive(Deserialize)]
pub(crate) struct TeamSaveForm {
    pub(crate) original_name: String,
    pub(crate) yaml: String,
}

#[derive(Deserialize)]
pub(crate) struct ScheduleActionForm {
    pub(crate) intent: String,
    pub(crate) original_name: Option<String>,
    pub(crate) name: Option<String>,
    pub(crate) cron_expression: Option<String>,
    pub(crate) team_name: Option<String>,
    pub(crate) input: Option<String>,
    pub(crate) enabled: Option<String>,
    pub(crate) confirm_delete: Option<String>,
}

#[derive(Deserialize)]
pub(crate) struct TriggerActionForm {
    pub(crate) intent: String,
    pub(crate) original_name: Option<String>,
    pub(crate) name: Option<String>,
    pub(crate) trigger_type: Option<String>,
    pub(crate) team_name: Option<String>,
    pub(crate) condition_json: Option<String>,
    pub(crate) input: Option<String>,
}

#[derive(Deserialize)]
pub(crate) struct SessionActionForm {
    pub(crate) intent: String,
    pub(crate) session_key: String,
    pub(crate) selected_model: Option<String>,
}

#[derive(Deserialize, Default)]
pub(crate) struct TriggerWorkflowBody {
    pub(crate) input: Option<String>,
}
