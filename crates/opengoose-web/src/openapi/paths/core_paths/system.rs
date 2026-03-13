/// System paths: health checks, readiness, liveness, metrics.
use serde_json::{Value, json};

pub(in crate::openapi) fn build() -> Value {
    json!({
        "/api/health": {
            "get": {
                "tags": ["system"],
                "summary": "Health check",
                "operationId": "getHealth",
                "responses": {
                    "200": {
                        "description": "Current service health snapshot",
                        "content": {
                            "application/json": {
                                "schema": { "$ref": "#/components/schemas/HealthResponse" }
                            }
                        }
                    }
                }
            }
        },
        "/api/health/ready": {
            "get": {
                "tags": ["system"],
                "summary": "Readiness probe",
                "operationId": "getReadiness",
                "responses": {
                    "200": {
                        "description": "All critical components are healthy",
                        "content": {
                            "application/json": {
                                "schema": { "$ref": "#/components/schemas/HealthResponse" }
                            }
                        }
                    },
                    "503": {
                        "description": "One or more critical components are not healthy",
                        "content": {
                            "application/json": {
                                "schema": { "$ref": "#/components/schemas/HealthResponse" }
                            }
                        }
                    }
                }
            }
        },
        "/api/health/live": {
            "get": {
                "tags": ["system"],
                "summary": "Liveness probe",
                "operationId": "getLiveness",
                "responses": {
                    "200": {
                        "description": "Process is alive"
                    }
                }
            }
        },
        "/api/metrics": {
            "get": {
                "tags": ["system"],
                "summary": "Prometheus metrics",
                "operationId": "getMetrics",
                "responses": {
                    "200": {
                        "description": "Prometheus exposition format",
                        "content": {
                            "text/plain": {}
                        }
                    }
                }
            }
        }
    })
}
