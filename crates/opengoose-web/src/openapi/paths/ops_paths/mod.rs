/// Operations API paths grouped by domain-specific submodules.
mod alerts;
mod channels;
mod events;
mod gateways;
mod triggers;
mod webhooks;

use serde_json::{Map, Value};

pub(in crate::openapi) fn build() -> Value {
    let mut paths = Map::new();

    extend_paths(&mut paths, alerts::build());
    extend_paths(&mut paths, triggers::build());
    extend_paths(&mut paths, channels::build());
    extend_paths(&mut paths, gateways::build());
    extend_paths(&mut paths, webhooks::build());
    extend_paths(&mut paths, events::build());

    Value::Object(paths)
}

fn extend_paths(paths: &mut Map<String, Value>, module_paths: Value) {
    if let Value::Object(module_paths) = module_paths {
        paths.extend(module_paths);
    }
}

#[cfg(test)]
mod tests;
