use diesel::prelude::*;
use diesel::sqlite::SqliteConnection;

use super::types::OrchestrationRun;
use crate::error::PersistenceResult;
use crate::models::OrchestrationRunRow;
use crate::run_status::RunStatus;
use crate::schema::orchestration_runs;

pub fn get_run(
    conn: &mut SqliteConnection,
    team_run_id: &str,
) -> PersistenceResult<Option<OrchestrationRun>> {
    let result = orchestration_runs::table
        .filter(orchestration_runs::team_run_id.eq(team_run_id))
        .first::<OrchestrationRunRow>(conn)
        .optional()?;

    match result {
        Some(row) => Ok(Some(OrchestrationRun::from_row(row)?)),
        None => Ok(None),
    }
}

pub fn list_runs(
    conn: &mut SqliteConnection,
    status: Option<&RunStatus>,
    limit: i64,
) -> PersistenceResult<Vec<OrchestrationRun>> {
    let mut query = orchestration_runs::table
        .order(orchestration_runs::updated_at.desc())
        .limit(limit)
        .into_boxed();

    if let Some(status) = status {
        query = query.filter(orchestration_runs::status.eq(status.as_str()));
    }

    let rows = query.load::<OrchestrationRunRow>(conn)?;
    rows.into_iter()
        .map(OrchestrationRun::from_row)
        .collect::<Result<_, _>>()
}

pub fn count_runs(conn: &mut SqliteConnection) -> PersistenceResult<i64> {
    let count = orchestration_runs::table
        .count()
        .get_result::<i64>(conn)?;
    Ok(count)
}

pub fn find_suspended(
    conn: &mut SqliteConnection,
    session_key: &str,
) -> PersistenceResult<Vec<OrchestrationRun>> {
    let rows = orchestration_runs::table
        .filter(orchestration_runs::session_key.eq(session_key))
        .filter(orchestration_runs::status.eq(RunStatus::Suspended.as_str()))
        .order(orchestration_runs::updated_at.desc())
        .load::<OrchestrationRunRow>(conn)?;

    rows.into_iter()
        .map(OrchestrationRun::from_row)
        .collect::<Result<_, _>>()
}
