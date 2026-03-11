use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

/// Shared health states used by API probes.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum HealthStatus {
    Healthy,
    Degraded,
    Unavailable,
}

/// A point-in-time health check result for a single component.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ComponentHealth {
    pub status: HealthStatus,
    pub last_check: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error_detail: Option<String>,
}

/// Structured component health for the system health endpoint.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct HealthComponents {
    pub database: ComponentHealth,
    pub cron_scheduler: ComponentHealth,
    pub alert_dispatcher: ComponentHealth,
    pub gateways: BTreeMap<String, ComponentHealth>,
}

/// Response body for the full system health endpoint.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct HealthResponse {
    pub status: HealthStatus,
    pub version: String,
    pub checked_at: String,
    pub components: HealthComponents,
}

/// Lightweight process probe response used by liveness checks.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ServiceProbeResponse {
    pub status: HealthStatus,
    pub checked_at: String,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn health_status_serializes_as_snake_case() {
        let json =
            serde_json::to_string(&HealthStatus::Unavailable).expect("status should serialize");
        assert_eq!(json, "\"unavailable\"");
    }

    #[test]
    fn component_health_omits_error_detail_when_empty() {
        let component = ComponentHealth {
            status: HealthStatus::Healthy,
            last_check: "2026-03-11T00:00:00Z".into(),
            error_detail: None,
        };

        let json = serde_json::to_value(component).expect("component should serialize");

        assert!(json.get("error_detail").is_none());
    }

    #[test]
    fn health_response_preserves_gateway_map_order() {
        let response = HealthResponse {
            status: HealthStatus::Degraded,
            version: "0.1.0".into(),
            checked_at: "2026-03-11T00:00:00Z".into(),
            components: HealthComponents {
                database: ComponentHealth {
                    status: HealthStatus::Healthy,
                    last_check: "2026-03-11T00:00:00Z".into(),
                    error_detail: None,
                },
                cron_scheduler: ComponentHealth {
                    status: HealthStatus::Healthy,
                    last_check: "2026-03-11T00:00:00Z".into(),
                    error_detail: None,
                },
                alert_dispatcher: ComponentHealth {
                    status: HealthStatus::Healthy,
                    last_check: "2026-03-11T00:00:00Z".into(),
                    error_detail: None,
                },
                gateways: BTreeMap::from([
                    (
                        "discord".into(),
                        ComponentHealth {
                            status: HealthStatus::Healthy,
                            last_check: "2026-03-11T00:00:00Z".into(),
                            error_detail: None,
                        },
                    ),
                    (
                        "slack".into(),
                        ComponentHealth {
                            status: HealthStatus::Degraded,
                            last_check: "2026-03-11T00:00:00Z".into(),
                            error_detail: Some("timeout".into()),
                        },
                    ),
                ]),
            },
        };

        let json = serde_json::to_value(response).expect("response should serialize");
        let gateways = json["components"]["gateways"]
            .as_object()
            .expect("gateways should be an object");

        let keys: Vec<&str> = gateways.keys().map(String::as_str).collect();
        assert_eq!(keys, vec!["discord", "slack"]);
    }
}
