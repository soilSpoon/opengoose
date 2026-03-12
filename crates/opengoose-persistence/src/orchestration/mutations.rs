use diesel::prelude::*;
use diesel::sqlite::SqliteConnection;

use crate::db;
use crate::error::{PersistenceError, PersistenceResult};
use crate::models::{NewOrchestrationRun, NewSession};
use crate::run_status::RunStatus;
use crate::schema::{orchestration_runs, sessions};

pub fn create_run(
    conn: &mut SqliteConnection,
    team_run_id: &str,
    session_key: &str,
    team_name: &str,
    workflow: &str,
    input: &str,
    total_steps: i32,
) -> PersistenceResult<()> {
    conn.transaction(|conn| {
        diesel::insert_into(sessions::table)
            .values(NewSession {
                session_key,
                selected_model: None,
            })
            .on_conflict(sessions::session_key)
            .do_nothing()
            .execute(conn)?;

        diesel::insert_into(orchestration_runs::table)
            .values(NewOrchestrationRun {
                team_run_id,
                session_key,
                team_name,
                workflow,
                input,
                total_steps,
            })
            .execute(conn)?;
        Ok::<(), PersistenceError>(())
    })?;

    Ok(())
}

pub fn advance_step(
    conn: &mut SqliteConnection,
    team_run_id: &str,
    step: i32,
) -> PersistenceResult<()> {
    diesel::update(
        orchestration_runs::table.filter(orchestration_runs::team_run_id.eq(team_run_id)),
    )
    .set((
        orchestration_runs::current_step.eq(step),
        orchestration_runs::updated_at.eq(db::now_sql()),
    ))
    .execute(conn)?;
    Ok(())
}

pub fn resume_run(conn: &mut SqliteConnection, team_run_id: &str) -> PersistenceResult<()> {
    diesel::update(
        orchestration_runs::table.filter(orchestration_runs::team_run_id.eq(team_run_id)),
    )
    .set((
        orchestration_runs::status.eq(RunStatus::Running.as_str()),
        orchestration_runs::updated_at.eq(db::now_sql()),
    ))
    .execute(conn)?;
    Ok(())
}

pub fn complete_run(
    conn: &mut SqliteConnection,
    team_run_id: &str,
    result: &str,
) -> PersistenceResult<()> {
    diesel::update(
        orchestration_runs::table.filter(orchestration_runs::team_run_id.eq(team_run_id)),
    )
    .set((
        orchestration_runs::status.eq(RunStatus::Completed.as_str()),
        orchestration_runs::result.eq(Some(result)),
        orchestration_runs::updated_at.eq(db::now_sql()),
    ))
    .execute(conn)?;
    Ok(())
}

pub fn fail_run(
    conn: &mut SqliteConnection,
    team_run_id: &str,
    error: &str,
) -> PersistenceResult<()> {
    diesel::update(
        orchestration_runs::table.filter(orchestration_runs::team_run_id.eq(team_run_id)),
    )
    .set((
        orchestration_runs::status.eq(RunStatus::Failed.as_str()),
        orchestration_runs::result.eq(Some(error)),
        orchestration_runs::updated_at.eq(db::now_sql()),
    ))
    .execute(conn)?;
    Ok(())
}

pub fn suspend_incomplete(conn: &mut SqliteConnection) -> PersistenceResult<usize> {
    let count = diesel::update(
        orchestration_runs::table
            .filter(orchestration_runs::status.eq(RunStatus::Running.as_str())),
    )
    .set((
        orchestration_runs::status.eq(RunStatus::Suspended.as_str()),
        orchestration_runs::updated_at.eq(db::now_sql()),
    ))
    .execute(conn)?;

    Ok(count)
}
