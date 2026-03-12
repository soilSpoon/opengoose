use std::sync::Arc;

use diesel::prelude::*;
use tracing::debug;

use crate::db::{self, Database};
use crate::error::PersistenceResult;
use crate::models::{NewSchedule, ScheduleRow};
use crate::schema::schedules;

/// A cron schedule for automatic team execution.
#[derive(Debug, Clone)]
pub struct Schedule {
    pub id: i32,
    pub name: String,
    pub cron_expression: String,
    pub team_name: String,
    pub input: String,
    pub enabled: bool,
    pub last_run_at: Option<String>,
    pub next_run_at: Option<String>,
    pub created_at: String,
    pub updated_at: String,
}

impl Schedule {
    fn from_row(row: ScheduleRow) -> Self {
        Self {
            id: row.id,
            name: row.name,
            cron_expression: row.cron_expression,
            team_name: row.team_name,
            input: row.input,
            enabled: row.enabled != 0,
            last_run_at: row.last_run_at,
            next_run_at: row.next_run_at,
            created_at: row.created_at,
            updated_at: row.updated_at,
        }
    }
}

/// CRUD operations on the `schedules` table.
pub struct ScheduleStore {
    db: Arc<Database>,
}

pub struct ScheduleUpdate<'a> {
    pub name: &'a str,
    pub cron_expression: &'a str,
    pub team_name: &'a str,
    pub input: &'a str,
    pub enabled: bool,
    pub next_run_at: Option<&'a str>,
}

impl ScheduleStore {
    pub fn new(db: Arc<Database>) -> Self {
        Self { db }
    }

    /// Create a new schedule.
    pub fn create(
        &self,
        name: &str,
        cron_expression: &str,
        team_name: &str,
        input: &str,
        next_run_at: Option<&str>,
    ) -> PersistenceResult<Schedule> {
        self.db.with(|conn| {
            diesel::insert_into(schedules::table)
                .values(NewSchedule {
                    name,
                    cron_expression,
                    team_name,
                    input,
                    next_run_at,
                })
                .execute(conn)?;

            let row = schedules::table
                .filter(schedules::name.eq(name))
                .first::<ScheduleRow>(conn)?;

            debug!(name, team_name, cron_expression, "schedule created");
            Ok(Schedule::from_row(row))
        })
    }

    /// List all schedules.
    pub fn list(&self) -> PersistenceResult<Vec<Schedule>> {
        self.db.with(|conn| {
            let rows = schedules::table
                .order(schedules::name.asc())
                .load::<ScheduleRow>(conn)?;
            Ok(rows.into_iter().map(Schedule::from_row).collect())
        })
    }

    /// Get a schedule by name.
    pub fn get_by_name(&self, name: &str) -> PersistenceResult<Option<Schedule>> {
        self.db.with(|conn| {
            let result = schedules::table
                .filter(schedules::name.eq(name))
                .first::<ScheduleRow>(conn)
                .optional()?;
            Ok(result.map(Schedule::from_row))
        })
    }

    /// Remove a schedule by name.
    pub fn remove(&self, name: &str) -> PersistenceResult<bool> {
        self.db.with(|conn| {
            let count =
                diesel::delete(schedules::table.filter(schedules::name.eq(name))).execute(conn)?;
            if count > 0 {
                debug!(name, "schedule removed");
            }
            Ok(count > 0)
        })
    }

    /// Enable a schedule.
    pub fn set_enabled(&self, name: &str, enabled: bool) -> PersistenceResult<bool> {
        self.db.with(|conn| {
            let count = diesel::update(schedules::table.filter(schedules::name.eq(name)))
                .set((
                    schedules::enabled.eq(if enabled { 1 } else { 0 }),
                    schedules::updated_at.eq(db::now_sql()),
                ))
                .execute(conn)?;
            Ok(count > 0)
        })
    }

    /// Update an existing schedule in place.
    pub fn update(
        &self,
        original_name: &str,
        update: ScheduleUpdate<'_>,
    ) -> PersistenceResult<bool> {
        self.db.with(|conn| {
            let count = diesel::update(schedules::table.filter(schedules::name.eq(original_name)))
                .set((
                    schedules::name.eq(update.name),
                    schedules::cron_expression.eq(update.cron_expression),
                    schedules::team_name.eq(update.team_name),
                    schedules::input.eq(update.input),
                    schedules::enabled.eq(if update.enabled { 1 } else { 0 }),
                    schedules::next_run_at.eq(update.next_run_at),
                    schedules::updated_at.eq(db::now_sql()),
                ))
                .execute(conn)?;
            if count > 0 {
                debug!(original_name, name = update.name, "schedule updated");
            }
            Ok(count > 0)
        })
    }

    /// Record that a schedule was run and compute next run time.
    pub fn mark_run(&self, name: &str, next_run_at: Option<&str>) -> PersistenceResult<bool> {
        self.db.with(|conn| {
            let count = diesel::update(schedules::table.filter(schedules::name.eq(name)))
                .set((
                    schedules::last_run_at.eq(db::now_sql_nullable()),
                    schedules::next_run_at.eq(next_run_at),
                    schedules::updated_at.eq(db::now_sql()),
                ))
                .execute(conn)?;
            Ok(count > 0)
        })
    }

    /// List schedules that are enabled and due (next_run_at <= now).
    pub fn list_due(&self) -> PersistenceResult<Vec<Schedule>> {
        self.db.with(|conn| {
            let rows = schedules::table
                .filter(schedules::enabled.eq(1))
                .filter(schedules::next_run_at.le(db::now_sql_nullable()))
                .order(schedules::next_run_at.asc())
                .load::<ScheduleRow>(conn)?;
            Ok(rows.into_iter().map(Schedule::from_row).collect())
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_helpers::test_db;

    #[test]
    fn test_create_and_get() {
        let db = test_db();
        let store = ScheduleStore::new(db);

        let schedule = store
            .create(
                "nightly-review",
                "0 0 * * *",
                "code-review",
                "review all PRs",
                None,
            )
            .unwrap();

        assert_eq!(schedule.name, "nightly-review");
        assert_eq!(schedule.team_name, "code-review");
        assert!(schedule.enabled);

        let fetched = store.get_by_name("nightly-review").unwrap().unwrap();
        assert_eq!(fetched.cron_expression, "0 0 * * *");
    }

    #[test]
    fn test_list() {
        let db = test_db();
        let store = ScheduleStore::new(db);

        store
            .create("alpha", "0 * * * *", "team-a", "", None)
            .unwrap();
        store
            .create("beta", "0 0 * * *", "team-b", "", None)
            .unwrap();

        let all = store.list().unwrap();
        assert_eq!(all.len(), 2);
        assert_eq!(all[0].name, "alpha");
        assert_eq!(all[1].name, "beta");
    }

    #[test]
    fn test_remove() {
        let db = test_db();
        let store = ScheduleStore::new(db);

        store
            .create("temp", "0 * * * *", "team-a", "", None)
            .unwrap();
        assert!(store.remove("temp").unwrap());
        assert!(!store.remove("temp").unwrap());
        assert!(store.get_by_name("temp").unwrap().is_none());
    }

    #[test]
    fn test_enable_disable() {
        let db = test_db();
        let store = ScheduleStore::new(db);

        store
            .create("sched", "0 * * * *", "team-a", "", None)
            .unwrap();

        store.set_enabled("sched", false).unwrap();
        let s = store.get_by_name("sched").unwrap().unwrap();
        assert!(!s.enabled);

        store.set_enabled("sched", true).unwrap();
        let s = store.get_by_name("sched").unwrap().unwrap();
        assert!(s.enabled);
    }

    #[test]
    fn test_update() {
        let db = test_db();
        let store = ScheduleStore::new(db);

        store
            .create(
                "sched",
                "0 * * * *",
                "team-a",
                "",
                Some("2026-01-01 00:00:00"),
            )
            .unwrap();

        assert!(
            store
                .update(
                    "sched",
                    ScheduleUpdate {
                        name: "sched",
                        cron_expression: "0 30 * * * *",
                        team_name: "team-b",
                        input: "ship it",
                        enabled: false,
                        next_run_at: None,
                    },
                )
                .unwrap()
        );

        let updated = store.get_by_name("sched").unwrap().unwrap();
        assert_eq!(updated.cron_expression, "0 30 * * * *");
        assert_eq!(updated.team_name, "team-b");
        assert_eq!(updated.input, "ship it");
        assert!(!updated.enabled);
        assert!(updated.next_run_at.is_none());
    }

    #[test]
    fn test_mark_run() {
        let db = test_db();
        let store = ScheduleStore::new(db);

        store
            .create(
                "sched",
                "0 * * * *",
                "team-a",
                "",
                Some("2026-01-01 00:00:00"),
            )
            .unwrap();

        store
            .mark_run("sched", Some("2026-01-01 01:00:00"))
            .unwrap();

        let s = store.get_by_name("sched").unwrap().unwrap();
        assert!(s.last_run_at.is_some());
        assert_eq!(s.next_run_at.as_deref(), Some("2026-01-01 01:00:00"));
    }

    #[test]
    fn test_get_nonexistent() {
        let db = test_db();
        let store = ScheduleStore::new(db);
        assert!(store.get_by_name("no-such").unwrap().is_none());
    }

    #[test]
    fn test_list_due() {
        let db = test_db();
        let store = ScheduleStore::new(db);

        // Create a schedule with next_run_at in the past (should be due)
        store
            .create(
                "past-sched",
                "0 * * * *",
                "team-a",
                "",
                Some("2000-01-01 00:00:00"),
            )
            .unwrap();

        // Create a schedule with next_run_at in the far future (should not be due)
        store
            .create(
                "future-sched",
                "0 * * * *",
                "team-b",
                "",
                Some("2099-01-01 00:00:00"),
            )
            .unwrap();

        // Create a disabled schedule with past next_run_at (should not be due)
        store
            .create(
                "disabled-sched",
                "0 * * * *",
                "team-c",
                "",
                Some("2000-01-01 00:00:00"),
            )
            .unwrap();
        store.set_enabled("disabled-sched", false).unwrap();

        let due = store.list_due().unwrap();
        assert_eq!(due.len(), 1);
        assert_eq!(due[0].name, "past-sched");
    }

    #[test]
    fn test_set_enabled_nonexistent() {
        let db = test_db();
        let store = ScheduleStore::new(db);
        let result = store.set_enabled("no-such", false).unwrap();
        assert!(!result);
    }

    #[test]
    fn test_mark_run_nonexistent() {
        let db = test_db();
        let store = ScheduleStore::new(db);
        let result = store.mark_run("no-such", None).unwrap();
        assert!(!result);
    }
}
