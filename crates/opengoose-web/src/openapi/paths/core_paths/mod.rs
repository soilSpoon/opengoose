/// Core API paths: system, dashboard, sessions, runs, agents, teams, workflows.
mod resources;
mod sessions;
mod system;

#[cfg(test)]
mod tests;

use serde_json::Value;

pub(in crate::openapi) fn build() -> Value {
    let mut paths = serde_json::Map::new();

    for group in [system::build(), sessions::build(), resources::build()] {
        if let Value::Object(entries) = group {
            paths.extend(entries);
        }
    }

    Value::Object(paths)
}
