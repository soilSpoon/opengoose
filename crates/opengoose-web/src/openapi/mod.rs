/// OpenAPI 3.0 specification for the OpenGoose web dashboard JSON API.
///
/// Served as JSON at `GET /api/openapi.json`.
/// An embedded Swagger UI is available at `GET /api/docs`.
mod paths;
mod schemas;

#[cfg(test)]
mod tests;

use axum::http::header;
use axum::response::{Html, IntoResponse};
use serde_json::{Value, json};

/// Build the complete OpenAPI 3.0 spec as a JSON value.
pub fn build_spec() -> Value {
    json!({
        "openapi": "3.0.3",
        "info": {
            "title": "OpenGoose Web API",
            "version": env!("CARGO_PKG_VERSION"),
            "description": "JSON API for the OpenGoose autonomous agent orchestration dashboard. \
                Provides session management, run history, workflow control, alert rules, \
                trigger management, gateway status, and real-time SSE events.",
            "contact": {
                "name": "OpenGoose",
                "url": "https://github.com/soilSpoon/opengoose"
            },
            "license": {
                "name": "MIT"
            }
        },
        "tags": [
            { "name": "system", "description": "Health and metrics endpoints" },
            { "name": "dashboard", "description": "Aggregate dashboard statistics" },
            { "name": "sessions", "description": "Chat session and message management" },
            { "name": "runs", "description": "Orchestration run history" },
            { "name": "agents", "description": "Agent profile management" },
            { "name": "teams", "description": "Team definition management" },
            { "name": "workflows", "description": "Workflow definitions and manual triggers" },
            { "name": "alerts", "description": "Alert rule management and history" },
            { "name": "triggers", "description": "Trigger CRUD and test-fire operations" },
            { "name": "channels", "description": "Channel adapter metrics" },
            { "name": "gateways", "description": "Gateway platform health status" },
            { "name": "webhooks", "description": "Inbound webhook receiver" },
            { "name": "events", "description": "Server-Sent Events stream" }
        ],
        "paths": paths::build_paths(),
        "components": {
            "responses": schemas::common_responses(),
            "schemas": schemas::build_schemas()
        }
    })
}

/// Swagger UI HTML page that loads the spec from `/api/openapi.json`.
const SWAGGER_UI_HTML: &str = r##"<!DOCTYPE html>
<html>
<head>
  <title>OpenGoose API — Swagger UI</title>
  <meta charset="utf-8"/>
  <meta name="viewport" content="width=device-width, initial-scale=1">
  <link rel="stylesheet" type="text/css" href="https://unpkg.com/swagger-ui-dist@5/swagger-ui.css">
</head>
<body>
  <div id="swagger-ui"></div>
  <script src="https://unpkg.com/swagger-ui-dist@5/swagger-ui-bundle.js"></script>
  <script>
    SwaggerUIBundle({
      url: "/api/openapi.json",
      dom_id: "#swagger-ui",
      presets: [SwaggerUIBundle.presets.apis, SwaggerUIBundle.SwaggerUIStandalonePreset],
      layout: "BaseLayout",
      deepLinking: true,
    });
  </script>
</body>
</html>"##;

/// `GET /api/openapi.json` — serve the OpenAPI 3.0 spec as JSON.
pub async fn serve_openapi_json() -> impl IntoResponse {
    let spec = build_spec();
    let json = serde_json::to_string_pretty(&spec).unwrap_or_else(|_| "{}".to_string());
    ([(header::CONTENT_TYPE, "application/json")], json)
}

/// `GET /api/docs` — serve the Swagger UI HTML page.
pub async fn serve_swagger_ui() -> Html<&'static str> {
    Html(SWAGGER_UI_HTML)
}
