use std::io::{self, IsTerminal};
use std::process::ExitCode;

use anyhow::Error;
use clap::Error as ClapError;
use serde::Serialize;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum OutputMode {
    Text,
    Json,
}

impl OutputMode {
    pub fn from_json_flag(json: bool) -> Self {
        if json { Self::Json } else { Self::Text }
    }

    pub fn is_json(self) -> bool {
        matches!(self, Self::Json)
    }
}

#[derive(Clone, Copy, Debug)]
pub struct CliOutput {
    mode: OutputMode,
    color: bool,
}

impl CliOutput {
    pub fn new(mode: OutputMode) -> Self {
        Self {
            mode,
            color: matches!(mode, OutputMode::Text) && io::stdout().is_terminal(),
        }
    }

    pub fn mode(self) -> OutputMode {
        self.mode
    }

    pub fn is_json(self) -> bool {
        self.mode.is_json()
    }

    pub fn heading(self, text: &str) -> String {
        self.paint(text, "1;36")
    }

    pub fn print_json<T: Serialize>(self, value: &T) -> anyhow::Result<()> {
        println!("{}", serde_json::to_string_pretty(value)?);
        Ok(())
    }

    fn paint(self, text: &str, ansi: &str) -> String {
        if self.color {
            format!("\u{1b}[{ansi}m{text}\u{1b}[0m")
        } else {
            text.to_string()
        }
    }
}

pub fn format_table(headers: &[&str], rows: &[Vec<String>]) -> String {
    let mut widths: Vec<usize> = headers.iter().map(|header| header.len()).collect();
    for row in rows {
        for (idx, cell) in row.iter().enumerate() {
            if idx >= widths.len() {
                widths.push(cell.len());
            } else {
                widths[idx] = widths[idx].max(cell.len());
            }
        }
    }

    let mut output = String::new();
    push_table_row(
        &mut output,
        &headers
            .iter()
            .map(|header| (*header).to_string())
            .collect::<Vec<_>>(),
        &widths,
    );

    let separator_len = widths.iter().sum::<usize>() + 2 * widths.len().saturating_sub(1);
    output.push_str(&"-".repeat(separator_len));
    output.push('\n');

    for row in rows {
        push_table_row(&mut output, row, &widths);
    }

    output
}

fn push_table_row(output: &mut String, row: &[String], widths: &[usize]) {
    for (idx, cell) in row.iter().enumerate() {
        if idx > 0 {
            output.push_str("  ");
        }
        let width = widths.get(idx).copied().unwrap_or_default();
        output.push_str(&format!("{cell:<width$}"));
    }
    output.push('\n');
}

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
            eprintln!(
                "{}",
                serde_json::to_string_pretty(&payload).expect("error JSON should serialize")
            );
        }
    }
}

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
        eprintln!(
            "{}",
            serde_json::to_string_pretty(&payload).expect("clap error JSON should serialize")
        );
    } else {
        let _ = err.print();
    }

    exit_code
}

struct FriendlyError {
    kind: &'static str,
    message: String,
    suggestion: Option<&'static str>,
}

impl FriendlyError {
    fn from_error(err: &Error) -> Self {
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
