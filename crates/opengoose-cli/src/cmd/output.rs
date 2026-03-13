use std::io::{self, IsTerminal};
use std::process::ExitCode;

use clap::Error as ClapError;
use serde::Serialize;

use crate::error::CliResult;

/// Whether the CLI should emit human-readable text or machine-readable JSON.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum OutputMode {
    /// Human-readable text with optional ANSI color.
    Text,
    /// Pretty-printed JSON on stdout.
    Json,
}

impl OutputMode {
    /// Select output mode from the `--json` flag value.
    pub fn from_json_flag(json: bool) -> Self {
        if json { Self::Json } else { Self::Text }
    }

    /// Returns `true` when JSON output is selected.
    pub fn is_json(self) -> bool {
        matches!(self, Self::Json)
    }
}

/// Holds the selected output mode and terminal capabilities for the current invocation.
#[derive(Clone, Copy, Debug)]
pub struct CliOutput {
    mode: OutputMode,
    color: bool,
}

impl CliOutput {
    /// Create a new output context, detecting color support for text mode.
    pub fn new(mode: OutputMode) -> Self {
        Self {
            mode,
            color: matches!(mode, OutputMode::Text) && io::stdout().is_terminal(),
        }
    }

    /// Return the active output mode.
    pub fn mode(self) -> OutputMode {
        self.mode
    }

    /// Returns `true` when JSON output is selected.
    pub fn is_json(self) -> bool {
        self.mode.is_json()
    }

    /// Format `text` as a bold cyan heading (plain text when color is off).
    pub fn heading(self, text: &str) -> String {
        self.paint(text, "1;36")
    }

    /// Pretty-print `value` as JSON to stdout.
    pub fn print_json<T: Serialize>(self, value: &T) -> CliResult<()> {
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

/// Format `headers` and `rows` as an aligned text table.
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
    let header_row: Vec<String> = headers.iter().map(|h| h.to_string()).collect();
    push_table_row(&mut output, &header_row, &widths);

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

/// Print a user-friendly error (with optional hint) to stderr in the active output mode.
pub fn print_error(output: CliOutput, err: &impl std::fmt::Display) {
    let friendly = FriendlyError::from_message(&err.to_string());

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

struct FriendlyError {
    kind: &'static str,
    message: String,
    suggestion: Option<&'static str>,
}

impl FriendlyError {
    fn from_message(message: &str) -> Self {
        let message = message.to_string();
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

#[cfg(test)]
mod tests {
    use super::*;
    use anyhow::anyhow;

    #[test]
    fn output_mode_selects_json() {
        assert!(OutputMode::from_json_flag(true).is_json());
        assert!(!OutputMode::from_json_flag(false).is_json());
    }

    #[test]
    fn heading_uses_color_only_for_terminal_text_mode() {
        let output = CliOutput::new(OutputMode::Text);
        assert_eq!(output.heading("dashboard"), "dashboard");
    }

    #[test]
    fn format_table_respects_column_widths() {
        let rows = vec![
            vec!["alpha".to_string(), "1".to_string()],
            vec!["beta".to_string(), "42".to_string()],
        ];
        let table = format_table(&["NAME", "VALUE"], &rows);

        let mut lines = table.lines();
        assert_eq!(lines.next().unwrap(), "NAME   VALUE");
        assert!(lines.next().unwrap().len() >= 8);
        assert!(lines.next().unwrap().contains("alpha"));
        assert!(lines.next().unwrap().contains("beta"));
    }

    #[test]
    fn friendly_error_maps_unknown_provider_to_invalid_input() {
        let err = anyhow!("unknown provider: definitely-unknown-provider");
        let friendly = FriendlyError::from_message(&err.to_string());
        assert_eq!(friendly.kind, "invalid_input");
        assert_eq!(
            friendly.message,
            "unknown provider: definitely-unknown-provider"
        );
        assert!(friendly.suggestion.is_some());
    }

    #[test]
    fn friendly_error_maps_file_not_found() {
        let err = anyhow!("file not found: /some/path.yaml");
        let friendly = FriendlyError::from_message(&err.to_string());
        assert_eq!(friendly.kind, "not_found");
        assert!(friendly.suggestion.is_some());
    }

    #[test]
    fn friendly_error_maps_profile_not_found() {
        let err = anyhow!("profile `developer` not found");
        let friendly = FriendlyError::from_message(&err.to_string());
        assert_eq!(friendly.kind, "not_found");
        assert!(friendly.suggestion.is_some());
    }

    #[test]
    fn friendly_error_maps_team_not_found() {
        let err = anyhow!("team `code-review` not found");
        let friendly = FriendlyError::from_message(&err.to_string());
        assert_eq!(friendly.kind, "not_found");
        assert!(friendly.suggestion.is_some());
    }

    #[test]
    fn friendly_error_maps_invalid_selection() {
        let err = anyhow!("invalid selection");
        let friendly = FriendlyError::from_message(&err.to_string());
        assert_eq!(friendly.kind, "invalid_input");
        assert!(friendly.suggestion.is_some());
    }

    #[test]
    fn friendly_error_maps_empty_value() {
        let err = anyhow!("empty value — aborting");
        let friendly = FriendlyError::from_message(&err.to_string());
        assert_eq!(friendly.kind, "invalid_input");
        assert!(friendly.suggestion.is_some());
    }

    #[test]
    fn friendly_error_maps_unsupported_output() {
        let err = anyhow!("`opengoose run` does not support --json");
        let friendly = FriendlyError::from_message(&err.to_string());
        assert_eq!(friendly.kind, "unsupported_output");
        assert!(friendly.suggestion.is_some());
    }

    #[test]
    fn friendly_error_defaults_to_runtime_error() {
        let err = anyhow!("some unexpected internal failure");
        let friendly = FriendlyError::from_message(&err.to_string());
        assert_eq!(friendly.kind, "runtime_error");
        assert!(friendly.suggestion.is_none());
    }

    #[test]
    fn format_table_empty_rows() {
        let table = format_table(&["NAME", "VALUE"], &[]);
        assert!(table.contains("NAME"));
        assert!(table.contains("VALUE"));
    }

    #[test]
    fn format_table_single_column() {
        let rows = vec![vec!["alpha".to_string()], vec!["beta".to_string()]];
        let table = format_table(&["MODEL"], &rows);
        assert!(table.contains("MODEL"));
        assert!(table.contains("alpha"));
        assert!(table.contains("beta"));
    }

    #[test]
    fn cli_output_mode_accessor() {
        let text = CliOutput::new(OutputMode::Text);
        assert_eq!(text.mode(), OutputMode::Text);

        let json = CliOutput::new(OutputMode::Json);
        assert_eq!(json.mode(), OutputMode::Json);
    }

    #[test]
    fn cli_output_is_json() {
        let text = CliOutput::new(OutputMode::Text);
        assert!(!text.is_json());

        let json = CliOutput::new(OutputMode::Json);
        assert!(json.is_json());
    }

    #[test]
    fn json_output_disables_color() {
        let json = CliOutput::new(OutputMode::Json);
        // In JSON mode, heading should return plain text (no ANSI codes)
        let heading = json.heading("test");
        assert_eq!(heading, "test");
        assert!(!heading.contains("\u{1b}"));
    }

    #[test]
    fn format_table_cell_wider_than_header() {
        let rows = vec![vec![
            "a-very-long-model-name".to_string(),
            "100".to_string(),
        ]];
        let table = format_table(&["NAME", "COUNT"], &rows);
        let lines: Vec<&str> = table.lines().collect();
        // header row should be padded to match the wider cell
        assert!(lines[0].contains("NAME"));
        assert!(lines[2].contains("a-very-long-model-name"));
    }

    #[test]
    fn format_table_row_has_more_columns_than_headers() {
        let rows = vec![vec![
            "alpha".to_string(),
            "1".to_string(),
            "extra".to_string(),
        ]];
        let table = format_table(&["NAME", "VALUE"], &rows);
        // Extra columns should still appear
        assert!(table.contains("extra"));
    }

    #[test]
    fn format_table_row_has_fewer_columns_than_headers() {
        let rows = vec![vec!["alpha".to_string()]];
        let table = format_table(&["NAME", "VALUE", "DESC"], &rows);
        assert!(table.contains("NAME"));
        assert!(table.contains("alpha"));
    }

    #[test]
    fn print_json_produces_valid_output() {
        let output = CliOutput::new(OutputMode::Json);
        let payload = serde_json::json!({"ok": true, "data": [1, 2, 3]});
        // Should not error
        output.print_json(&payload).unwrap();
    }

    #[test]
    fn friendly_error_selection_out_of_range() {
        let err = anyhow!("selection out of range: 99");
        let friendly = FriendlyError::from_message(&err.to_string());
        assert_eq!(friendly.kind, "invalid_input");
        assert!(friendly.suggestion.is_some());
    }

    #[test]
    fn print_error_text_mode_does_not_panic() {
        let output = CliOutput::new(OutputMode::Text);
        let err = anyhow!("something went wrong");
        // Should write to stderr without panicking
        print_error(output, &err);
    }

    #[test]
    fn print_error_json_mode_does_not_panic() {
        let output = CliOutput::new(OutputMode::Json);
        let err = anyhow!("something went wrong");
        print_error(output, &err);
    }

    #[test]
    fn print_error_with_friendly_suggestion() {
        let output = CliOutput::new(OutputMode::Text);
        let err = anyhow!("unknown provider: foobar");
        // Should produce a hint in text mode without panicking
        print_error(output, &err);
    }

    #[test]
    fn format_table_separator_length_matches_columns() {
        let rows = vec![vec!["abc".to_string(), "de".to_string()]];
        let table = format_table(&["COL1", "COL2"], &rows);
        let lines: Vec<&str> = table.lines().collect();
        // separator line (line index 1)
        let separator = lines[1];
        assert!(separator.chars().all(|c| c == '-'));
        // separator length = sum of widths + 2*(n_cols-1) for spacing
        let expected_len = 4 + 4 + 2; // COL1(4) + COL2(4) + 2 spacing
        assert_eq!(separator.len(), expected_len);
    }
}
