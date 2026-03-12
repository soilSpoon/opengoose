use anyhow::anyhow;

use super::errors::FriendlyError;
use super::print_error;
use super::table::format_table;
use super::types::{CliOutput, OutputMode};

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
    let friendly = FriendlyError::from_error(&err);
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
    let friendly = FriendlyError::from_error(&err);
    assert_eq!(friendly.kind, "not_found");
    assert!(friendly.suggestion.is_some());
}

#[test]
fn friendly_error_maps_profile_not_found() {
    let err = anyhow!("profile `developer` not found");
    let friendly = FriendlyError::from_error(&err);
    assert_eq!(friendly.kind, "not_found");
    assert!(friendly.suggestion.is_some());
}

#[test]
fn friendly_error_maps_team_not_found() {
    let err = anyhow!("team `code-review` not found");
    let friendly = FriendlyError::from_error(&err);
    assert_eq!(friendly.kind, "not_found");
    assert!(friendly.suggestion.is_some());
}

#[test]
fn friendly_error_maps_invalid_selection() {
    let err = anyhow!("invalid selection");
    let friendly = FriendlyError::from_error(&err);
    assert_eq!(friendly.kind, "invalid_input");
    assert!(friendly.suggestion.is_some());
}

#[test]
fn friendly_error_maps_empty_value() {
    let err = anyhow!("empty value — aborting");
    let friendly = FriendlyError::from_error(&err);
    assert_eq!(friendly.kind, "invalid_input");
    assert!(friendly.suggestion.is_some());
}

#[test]
fn friendly_error_maps_unsupported_output() {
    let err = anyhow!("`opengoose run` does not support --json");
    let friendly = FriendlyError::from_error(&err);
    assert_eq!(friendly.kind, "unsupported_output");
    assert!(friendly.suggestion.is_some());
}

#[test]
fn friendly_error_defaults_to_runtime_error() {
    let err = anyhow!("some unexpected internal failure");
    let friendly = FriendlyError::from_error(&err);
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
    let friendly = FriendlyError::from_error(&err);
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
