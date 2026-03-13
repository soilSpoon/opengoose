//! MCP Team Tools — a lightweight MCP Stdio server for structured inter-agent
//! communication in OpenGoose teams.
//!
//! When `communication_mode: mcp-tools` is set on a team definition, this
//! binary is spawned as a Goose extension. It exposes team communication
//! primitives (delegate, broadcast, send, read) over JSON-RPC on stdin/stdout.

use std::io::{self, BufRead, Write};
use std::sync::Arc;

use anyhow::{Result, anyhow};
use serde::{Deserialize, Serialize};

use opengoose_persistence::{AgentMessageStore, Database, MessageQueue, MessageType};

// ── JSON-RPC types ─────────────────────────────────────────────────────────

#[derive(Deserialize)]
struct JsonRpcRequest {
    jsonrpc: String,
    id: Option<serde_json::Value>,
    method: String,
    #[serde(default)]
    params: serde_json::Value,
}

#[derive(Serialize)]
struct JsonRpcResponse {
    jsonrpc: String,
    id: serde_json::Value,
    #[serde(skip_serializing_if = "Option::is_none")]
    result: Option<serde_json::Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    error: Option<JsonRpcError>,
}

#[derive(Serialize)]
struct JsonRpcError {
    code: i32,
    message: String,
}

// ── MCP protocol types ────────────────────────────────────────────────────

#[derive(Serialize)]
struct McpInitializeResult {
    #[serde(rename = "protocolVersion")]
    protocol_version: String,
    capabilities: McpCapabilities,
    #[serde(rename = "serverInfo")]
    server_info: McpServerInfo,
}

#[derive(Serialize)]
struct McpCapabilities {
    tools: McpToolsCapability,
}

#[derive(Serialize)]
struct McpToolsCapability {}

#[derive(Serialize)]
struct McpServerInfo {
    name: String,
    version: String,
}

#[derive(Serialize)]
struct McpTool {
    name: String,
    description: String,
    #[serde(rename = "inputSchema")]
    input_schema: serde_json::Value,
}

#[derive(Serialize)]
struct McpToolResult {
    content: Vec<McpContent>,
    #[serde(rename = "isError", skip_serializing_if = "Option::is_none")]
    is_error: Option<bool>,
}

#[derive(Serialize)]
struct McpContent {
    #[serde(rename = "type")]
    content_type: String,
    text: String,
}

// ── Environment configuration ──────────────────────────────────────────────

struct Config {
    db_path: String,
    agent_name: String,
    team_run_id: String,
    _team_members: Vec<String>,
}

impl Config {
    fn from_env() -> Result<Self> {
        let db_path =
            std::env::var("OPENGOOSE_DB_PATH").map_err(|_| anyhow!("OPENGOOSE_DB_PATH not set"))?;
        let agent_name = std::env::var("OPENGOOSE_AGENT_NAME")
            .map_err(|_| anyhow!("OPENGOOSE_AGENT_NAME not set"))?;
        let team_run_id = std::env::var("OPENGOOSE_TEAM_RUN_ID")
            .map_err(|_| anyhow!("OPENGOOSE_TEAM_RUN_ID not set"))?;
        let team_members = std::env::var("OPENGOOSE_TEAM_MEMBERS")
            .unwrap_or_default()
            .split(',')
            .filter(|s| !s.is_empty())
            .map(str::to_string)
            .collect();

        Ok(Config {
            db_path,
            agent_name,
            team_run_id,
            _team_members: team_members,
        })
    }
}

// ── Tool definitions ────────────────────────────────────────────────────────

fn tool_definitions() -> Vec<McpTool> {
    vec![
        McpTool {
            name: "team__delegate".into(),
            description: "Delegate a task to another agent on the team.".into(),
            input_schema: serde_json::json!({
                "type": "object",
                "properties": {
                    "agent": { "type": "string", "description": "Target agent name" },
                    "message": { "type": "string", "description": "Task or message to delegate" }
                },
                "required": ["agent", "message"]
            }),
        },
        McpTool {
            name: "team__broadcast".into(),
            description: "Broadcast a message to all team agents.".into(),
            input_schema: serde_json::json!({
                "type": "object",
                "properties": {
                    "message": { "type": "string", "description": "Message to broadcast" }
                },
                "required": ["message"]
            }),
        },
        McpTool {
            name: "team__read_broadcasts".into(),
            description: "Read recent broadcast messages from the team.".into(),
            input_schema: serde_json::json!({
                "type": "object",
                "properties": {
                    "since_id": { "type": "integer", "description": "Only return broadcasts after this ID" }
                }
            }),
        },
        McpTool {
            name: "team__send_message".into(),
            description: "Send a direct message to a specific team agent.".into(),
            input_schema: serde_json::json!({
                "type": "object",
                "properties": {
                    "to": { "type": "string", "description": "Recipient agent name" },
                    "message": { "type": "string", "description": "Message content" }
                },
                "required": ["to", "message"]
            }),
        },
        McpTool {
            name: "team__read_messages".into(),
            description: "Read messages addressed to this agent.".into(),
            input_schema: serde_json::json!({
                "type": "object",
                "properties": {}
            }),
        },
    ]
}

// ── Tool execution ──────────────────────────────────────────────────────────

fn execute_tool(
    tool_name: &str,
    params: &serde_json::Value,
    config: &Config,
    db: &Arc<Database>,
) -> McpToolResult {
    match execute_tool_inner(tool_name, params, config, db) {
        Ok(text) => McpToolResult {
            content: vec![McpContent {
                content_type: "text".into(),
                text,
            }],
            is_error: None,
        },
        Err(e) => McpToolResult {
            content: vec![McpContent {
                content_type: "text".into(),
                text: format!("Error: {e}"),
            }],
            is_error: Some(true),
        },
    }
}

fn execute_tool_inner(
    tool_name: &str,
    params: &serde_json::Value,
    config: &Config,
    db: &Arc<Database>,
) -> Result<String> {
    let msg_store = AgentMessageStore::new(db.clone());
    let queue = MessageQueue::new(db.clone());
    // Use team_run_id as session key for queue operations
    let session_key = &config.team_run_id;

    match tool_name {
        "team__delegate" => {
            let agent = params["agent"]
                .as_str()
                .ok_or_else(|| anyhow!("missing 'agent' parameter"))?;
            let message = params["message"]
                .as_str()
                .ok_or_else(|| anyhow!("missing 'message' parameter"))?;

            queue.enqueue(
                session_key,
                &config.team_run_id,
                &config.agent_name,
                agent,
                message,
                MessageType::Task,
            )?;
            Ok(format!("Delegated task to {agent}"))
        }
        "team__broadcast" => {
            let message = params["message"]
                .as_str()
                .ok_or_else(|| anyhow!("missing 'message' parameter"))?;

            msg_store.publish(session_key, &config.agent_name, "team", message)?;
            Ok("Broadcast sent".into())
        }
        "team__read_broadcasts" => {
            let messages = msg_store.list_recent_global(20)?;
            let filtered: Vec<_> = messages
                .into_iter()
                .filter(|m| m.to_agent.is_none())
                .collect();

            if filtered.is_empty() {
                return Ok("No broadcasts found.".into());
            }

            let text = filtered
                .iter()
                .map(|m| format!("[{}] {}: {}", m.created_at, m.from_agent, m.payload))
                .collect::<Vec<_>>()
                .join("\n");
            Ok(text)
        }
        "team__send_message" => {
            let to = params["to"]
                .as_str()
                .ok_or_else(|| anyhow!("missing 'to' parameter"))?;
            let message = params["message"]
                .as_str()
                .ok_or_else(|| anyhow!("missing 'message' parameter"))?;

            msg_store.send_directed(session_key, &config.agent_name, to, message)?;
            Ok(format!("Message sent to {to}"))
        }
        "team__read_messages" => {
            let messages = msg_store.list_for_agent(session_key, &config.agent_name, 20)?;
            if messages.is_empty() {
                return Ok("No messages.".into());
            }

            let text = messages
                .iter()
                .map(|m| format!("[{}] {}: {}", m.created_at, m.from_agent, m.payload))
                .collect::<Vec<_>>()
                .join("\n");
            Ok(text)
        }
        _ => Err(anyhow!("unknown tool: {tool_name}")),
    }
}

// ── Main loop ───────────────────────────────────────────────────────────────

fn main() -> Result<()> {
    let config = Config::from_env()?;
    let db = Arc::new(Database::open_at(std::path::PathBuf::from(
        &config.db_path,
    ))?);

    let stdin = io::stdin();
    let stdout = io::stdout();

    for line in stdin.lock().lines() {
        let line = line?;
        if line.trim().is_empty() {
            continue;
        }

        let request: JsonRpcRequest = match serde_json::from_str(&line) {
            Ok(req) => req,
            Err(e) => {
                let response = JsonRpcResponse {
                    jsonrpc: "2.0".into(),
                    id: serde_json::Value::Null,
                    result: None,
                    error: Some(JsonRpcError {
                        code: -32700,
                        message: format!("Parse error: {e}"),
                    }),
                };
                let mut out = stdout.lock();
                serde_json::to_writer(&mut out, &response)?;
                out.write_all(b"\n")?;
                out.flush()?;
                continue;
            }
        };

        if request.jsonrpc != "2.0" {
            continue;
        }

        let id = request.id.unwrap_or(serde_json::Value::Null);

        let response = match request.method.as_str() {
            "initialize" => JsonRpcResponse {
                jsonrpc: "2.0".into(),
                id,
                result: Some(serde_json::to_value(McpInitializeResult {
                    protocol_version: "2024-11-05".into(),
                    capabilities: McpCapabilities {
                        tools: McpToolsCapability {},
                    },
                    server_info: McpServerInfo {
                        name: "opengoose-team-tools".into(),
                        version: "0.1.0".into(),
                    },
                })?),
                error: None,
            },
            "notifications/initialized" => continue,
            "tools/list" => {
                let tools = tool_definitions();
                JsonRpcResponse {
                    jsonrpc: "2.0".into(),
                    id,
                    result: Some(serde_json::json!({ "tools": tools })),
                    error: None,
                }
            }
            "tools/call" => {
                let tool_name = request.params["name"].as_str().unwrap_or("").to_string();
                let arguments = request.params.get("arguments").cloned().unwrap_or_default();
                let tool_result = execute_tool(&tool_name, &arguments, &config, &db);
                JsonRpcResponse {
                    jsonrpc: "2.0".into(),
                    id,
                    result: Some(serde_json::to_value(tool_result)?),
                    error: None,
                }
            }
            _ => JsonRpcResponse {
                jsonrpc: "2.0".into(),
                id,
                result: None,
                error: Some(JsonRpcError {
                    code: -32601,
                    message: format!("Method not found: {}", request.method),
                }),
            },
        };

        let mut out = stdout.lock();
        serde_json::to_writer(&mut out, &response)?;
        out.write_all(b"\n")?;
        out.flush()?;
    }

    Ok(())
}
