/// Component schema definitions and reusable response components for the OpenAPI spec.
///
/// The module layout mirrors the OpenAPI domains already used in `paths/` so each
/// area can evolve without growing another monolithic JSON block.
mod agents;
mod alerts;
mod channels;
mod dashboard;
mod events;
mod gateways;
mod responses;
mod runs;
mod sessions;
mod system;
mod teams;
mod triggers;
mod workflows;

use serde_json::{Map, Value};

/// Common reusable response definitions (NotFound, InternalError, UnprocessableEntity).
pub(super) fn common_responses() -> Value {
    responses::build()
}

/// Build all component schema definitions by merging domain-specific schema groups.
pub(super) fn build_schemas() -> Value {
    let mut schemas = Map::new();

    for group in [
        system::build(),
        dashboard::build(),
        sessions::build(),
        runs::build(),
        agents::build(),
        teams::build(),
        workflows::build(),
        events::build(),
        alerts::build(),
        channels::build(),
        gateways::build(),
        triggers::build(),
    ] {
        if let Some(group) = group.as_object() {
            schemas.extend(group.clone());
        }
    }

    Value::Object(schemas)
}

#[cfg(test)]
mod tests;
