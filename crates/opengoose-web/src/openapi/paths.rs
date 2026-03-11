/// Endpoint path definitions for the OpenAPI spec, split across two submodules.
///
/// - `core`: system, dashboard, sessions, runs, agents, teams, workflows
/// - `ops`: alerts, triggers, channels, gateways, webhooks, events
pub(super) mod core_paths;
pub(super) mod ops_paths;

use serde_json::Value;

/// Build all API path definitions as a merged JSON object.
pub(super) fn build_paths() -> Value {
    let mut paths = core_paths::build()
        .as_object()
        .cloned()
        .unwrap_or_default();
    if let Some(ops) = ops_paths::build().as_object() {
        paths.extend(ops.clone());
    }
    Value::Object(paths)
}
