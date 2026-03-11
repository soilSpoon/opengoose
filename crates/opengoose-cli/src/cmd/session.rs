use std::sync::Arc;

use anyhow::{Result, anyhow, bail};
use clap::{Subcommand, ValueEnum};
use serde::Serialize;

use opengoose_persistence::{
    Database, SessionExport, SessionExportQuery, SessionStore, normalize_since_filter,
    normalize_until_filter, render_batch_session_exports_markdown, render_session_export_markdown,
};
use opengoose_types::SessionKey;

use crate::cmd::output::CliOutput;

#[derive(Clone, Copy, Debug, Eq, PartialEq, ValueEnum)]
pub enum ExportFormat {
    Json,
    Md,
}

#[derive(Subcommand)]
#[command(
    after_help = "Examples:\n  opengoose session export discord:ns:guild:channel --format json\n  opengoose session export discord:ns:guild:channel --format md\n  opengoose session export --since 7d --format json\n  opengoose session export --since 2026-03-01 --until 2026-03-10 --format md"
)]
pub enum SessionAction {
    /// Export a single session or a date-range batch as JSON or Markdown
    Export {
        /// Session key to export. Omit it to export a date-range batch instead.
        session: Option<String>,
        /// Output format for the export body.
        #[arg(long, value_enum, default_value_t = ExportFormat::Json)]
        format: ExportFormat,
        /// Only include sessions updated at or after this timestamp for batch export.
        #[arg(long)]
        since: Option<String>,
        /// Only include sessions updated at or before this timestamp for batch export.
        #[arg(long)]
        until: Option<String>,
        /// Maximum number of sessions to include in a batch export (default 100, max 1000).
        #[arg(long, default_value_t = 100)]
        limit: i64,
    },
}

#[derive(Serialize)]
struct BatchExportEnvelope<'a> {
    since: Option<&'a str>,
    until: Option<&'a str>,
    session_count: usize,
    sessions: &'a [SessionExport],
}

pub fn execute(action: SessionAction, output: CliOutput) -> Result<()> {
    if output.is_json() {
        bail!("`opengoose session export` uses `--format` and does not support --json");
    }

    match action {
        SessionAction::Export {
            session,
            format,
            since,
            until,
            limit,
        } => cmd_export(session, format, since, until, limit),
    }
}

fn cmd_export(
    session: Option<String>,
    format: ExportFormat,
    since: Option<String>,
    until: Option<String>,
    limit: i64,
) -> Result<()> {
    if limit <= 0 || limit > 1000 {
        bail!("`--limit` must be between 1 and 1000, got {limit}");
    }

    let store = SessionStore::new(Arc::new(Database::open()?));
    match session {
        Some(session) => export_single_session(&store, &session, format, since, until, limit),
        None => export_session_batch(&store, format, since, until, limit),
    }
}

fn export_single_session(
    store: &SessionStore,
    session: &str,
    format: ExportFormat,
    since: Option<String>,
    until: Option<String>,
    limit: i64,
) -> Result<()> {
    if session.trim().is_empty() {
        bail!("`session` must not be empty");
    }
    if since.is_some() || until.is_some() {
        bail!("`--since` and `--until` are only valid for batch export");
    }
    if limit != 100 {
        bail!("`--limit` is only valid for batch export");
    }

    let key = SessionKey::from_stable_id(session.trim());
    let export = store
        .export_session(&key)?
        .ok_or_else(|| anyhow!("session `{}` not found", session.trim()))?;

    emit_single_export(&export, format)
}

fn export_session_batch(
    store: &SessionStore,
    format: ExportFormat,
    since: Option<String>,
    until: Option<String>,
    limit: i64,
) -> Result<()> {
    if since.is_none() && until.is_none() {
        bail!("batch export requires at least one of `--since` or `--until`");
    }

    let since = since
        .as_deref()
        .map(normalize_since_filter)
        .transpose()
        .map_err(anyhow::Error::msg)?;
    let until = until
        .as_deref()
        .map(normalize_until_filter)
        .transpose()
        .map_err(anyhow::Error::msg)?;

    if let (Some(since), Some(until)) = (since.as_deref(), until.as_deref())
        && since > until
    {
        bail!("`--since` must be earlier than or equal to `--until`");
    }

    let exports = store.export_sessions(&SessionExportQuery {
        since: since.clone(),
        until: until.clone(),
        limit,
    })?;

    emit_batch_export(&exports, format, since.as_deref(), until.as_deref())
}

fn emit_single_export(export: &SessionExport, format: ExportFormat) -> Result<()> {
    match format {
        ExportFormat::Json => println!("{}", serde_json::to_string_pretty(export)?),
        ExportFormat::Md => print!("{}", render_session_export_markdown(export)),
    }

    Ok(())
}

fn emit_batch_export(
    exports: &[SessionExport],
    format: ExportFormat,
    since: Option<&str>,
    until: Option<&str>,
) -> Result<()> {
    match format {
        ExportFormat::Json => println!(
            "{}",
            serde_json::to_string_pretty(&BatchExportEnvelope {
                since,
                until,
                session_count: exports.len(),
                sessions: exports,
            })?
        ),
        ExportFormat::Md => print!(
            "{}",
            render_batch_session_exports_markdown(exports, since, until)
        ),
    }

    Ok(())
}
