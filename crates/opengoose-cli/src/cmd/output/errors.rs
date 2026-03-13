use std::process::ExitCode;

use anyhow::Error;
use clap::Error as ClapError;

use super::types::{CliOutput, OutputMode};

/// Print a user-friendly error (with optional hint) to stderr in the active output mode.
pub fn print_error(output: CliOutput, err: &Error) {
    let friendly = FriendlyError::from_error(err);

    match output.mode() {
        OutputMode::Text => {
            eprintln!("{} {}", output.heading("Error:"), friendly.message);
            if let Some(suggestion) = friendly.suggestion {
                eprintln!("{} {}", output.heading("Hint:"), suggestion);
            }
        }
        OutputMode::Json => {
            let payload = serde_json::json!({
                "ok": false,
                "error": {
                    "kind": friendly.kind,
                    "message": friendly.message,
                    "suggestion": friendly.suggestion,
                }
            });
            let serialized = serde_json::to_string_pretty(&payload)
                .unwrap_or_else(|_| r#"{"ok":false,"error":{"kind":"runtime_error","message":"(serialization failed)","suggestion":null}}"#.to_string());
            eprintln!("{}", serialized);
        }
    }
}

/// Print a clap argument-parsing error and return the appropriate exit code.
pub fn print_clap_error(requested_json: bool, err: ClapError) -> ExitCode {
    let exit_code = ExitCode::from(err.exit_code() as u8);

    if requested_json {
        let payload = serde_json::json!({
            "ok": false,
            "error": {
                "kind": "argument_error",
                "message": err.to_string().trim(),
                "suggestion": "Run `opengoose --help` to inspect the available commands and flags.",
            }
        });
        let serialized = serde_json::to_string_pretty(&payload)
            .unwrap_or_else(|_| r#"{"ok":false,"error":{"kind":"argument_error","message":"(serialization failed)","suggestion":null}}"#.to_string());
        eprintln!("{}", serialized);
    } else {
        let _ = err.print();
    }

    exit_code
}

pub(crate) struct FriendlyError {
    pub(crate) kind: &'static str,
    pub(crate) message: String,
    pub(crate) suggestion: Option<&'static str>,
}

impl FriendlyError {
    pub(crate) fn from_error(err: &Error) -> Self {
        let message = err.to_string();
        let lower = message.to_ascii_lowercase();

        if lower.contains("unknown provider") {
            return Self {
                kind: "invalid_input",
                message,
                suggestion: Some("Run `opengoose auth list` to see the supported providers."),
            };
        }

        if lower.starts_with("file not found:") {
            return Self {
                kind: "not_found",
                message,
                suggestion: Some("Check the file path and try again."),
            };
        }

        if lower.contains("profile `") && lower.contains("not found") {
            return Self {
                kind: "not_found",
                message,
                suggestion: Some("Run `opengoose profile list` to see the installed profiles."),
            };
        }

        if lower.contains("team `") && lower.contains("not found") {
            return Self {
                kind: "not_found",
                message,
                suggestion: Some("Run `opengoose team list` to see the installed teams."),
            };
        }

        if lower.contains("invalid selection") || lower.contains("selection out of range") {
            return Self {
                kind: "invalid_input",
                message,
                suggestion: Some("Enter one of the listed numbers from the prompt."),
            };
        }

        if lower.contains("empty value") {
            return Self {
                kind: "invalid_input",
                message,
                suggestion: Some("Provide a non-empty value and try again."),
            };
        }

        if lower.contains("does not support --json") {
            return Self {
                kind: "unsupported_output",
                message,
                suggestion: Some("Re-run the command without `--json`."),
            };
        }

        Self {
            kind: "runtime_error",
            message,
            suggestion: None,
        }
    }
}
