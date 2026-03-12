use std::io::{self, IsTerminal};

use serde::Serialize;

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
