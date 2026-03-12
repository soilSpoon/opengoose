use std::sync::Arc;

use diesel::prelude::*;
use tracing::debug;

use crate::db::{self, Database};
use crate::error::PersistenceResult;
use crate::models::{NewTrigger, TriggerRow};
use crate::schema::triggers;

/// An event trigger that fires a team run when conditions are met.
#[derive(Debug, Clone)]
pub struct Trigger {
    pub id: i32,
    pub name: String,
    pub trigger_type: String,
    pub condition_json: String,
    pub team_name: String,
    pub input: String,
    pub enabled: bool,
    pub last_fired_at: Option<String>,
    pub fire_count: i32,
    pub created_at: String,
    pub updated_at: String,
}

impl Trigger {
    fn from_row(row: TriggerRow) -> Self {
        Self {
            id: row.id,
            name: row.name,
            trigger_type: row.trigger_type,
            condition_json: row.condition_json,
            team_name: row.team_name,
            input: row.input,
            enabled: row.enabled != 0,
            last_fired_at: row.last_fired_at,
            fire_count: row.fire_count,
            created_at: row.created_at,
            updated_at: row.updated_at,
        }
    }
}

/// CRUD operations on the `triggers` table.
pub struct TriggerStore {
    db: Arc<Database>,
}

impl TriggerStore {
    pub fn new(db: Arc<Database>) -> Self {
        Self { db }
    }

    /// Create a new trigger.
    pub fn create(
        &self,
        name: &str,
        trigger_type: &str,
        condition_json: &str,
        team_name: &str,
        input: &str,
    ) -> PersistenceResult<Trigger> {
        self.db.with(|conn| {
            diesel::insert_into(triggers::table)
                .values(NewTrigger {
                    name,
                    trigger_type,
                    condition_json,
                    team_name,
                    input,
                })
                .execute(conn)?;

            let row = triggers::table
                .filter(triggers::name.eq(name))
                .first::<TriggerRow>(conn)?;

            debug!(name, trigger_type, team_name, "trigger created");
            Ok(Trigger::from_row(row))
        })
    }

    /// List all triggers.
    pub fn list(&self) -> PersistenceResult<Vec<Trigger>> {
        self.db.with(|conn| {
            let rows = triggers::table
                .order(triggers::name.asc())
                .load::<TriggerRow>(conn)?;
            Ok(rows.into_iter().map(Trigger::from_row).collect())
        })
    }

    /// List triggers by type.
    pub fn list_by_type(&self, trigger_type: &str) -> PersistenceResult<Vec<Trigger>> {
        self.db.with(|conn| {
            let rows = triggers::table
                .filter(triggers::trigger_type.eq(trigger_type))
                .filter(triggers::enabled.eq(1))
                .order(triggers::name.asc())
                .load::<TriggerRow>(conn)?;
            Ok(rows.into_iter().map(Trigger::from_row).collect())
        })
    }

    /// Get a trigger by name.
    pub fn get_by_name(&self, name: &str) -> PersistenceResult<Option<Trigger>> {
        self.db.with(|conn| {
            let result = triggers::table
                .filter(triggers::name.eq(name))
                .first::<TriggerRow>(conn)
                .optional()?;
            Ok(result.map(Trigger::from_row))
        })
    }

    /// Remove a trigger by name.
    pub fn remove(&self, name: &str) -> PersistenceResult<bool> {
        self.db.with(|conn| {
            let count =
                diesel::delete(triggers::table.filter(triggers::name.eq(name))).execute(conn)?;
            if count > 0 {
                debug!(name, "trigger removed");
            }
            Ok(count > 0)
        })
    }

    /// Update mutable fields of an existing trigger.
    pub fn update(
        &self,
        name: &str,
        trigger_type: &str,
        condition_json: &str,
        team_name: &str,
        input: &str,
    ) -> PersistenceResult<Option<Trigger>> {
        self.db.with(|conn| {
            let count = diesel::update(triggers::table.filter(triggers::name.eq(name)))
                .set((
                    triggers::trigger_type.eq(trigger_type),
                    triggers::condition_json.eq(condition_json),
                    triggers::team_name.eq(team_name),
                    triggers::input.eq(input),
                    triggers::updated_at.eq(db::now_sql()),
                ))
                .execute(conn)?;

            if count == 0 {
                return Ok(None);
            }

            let row = triggers::table
                .filter(triggers::name.eq(name))
                .first::<TriggerRow>(conn)?;

            debug!(name, trigger_type, team_name, "trigger updated");
            Ok(Some(Trigger::from_row(row)))
        })
    }

    /// Enable or disable a trigger.
    pub fn set_enabled(&self, name: &str, enabled: bool) -> PersistenceResult<bool> {
        self.db.with(|conn| {
            let count = diesel::update(triggers::table.filter(triggers::name.eq(name)))
                .set((
                    triggers::enabled.eq(if enabled { 1 } else { 0 }),
                    triggers::updated_at.eq(db::now_sql()),
                ))
                .execute(conn)?;
            Ok(count > 0)
        })
    }

    /// Record that a trigger fired.
    pub fn mark_fired(&self, name: &str) -> PersistenceResult<bool> {
        self.db.with(|conn| {
            let count = diesel::update(triggers::table.filter(triggers::name.eq(name)))
                .set((
                    triggers::last_fired_at.eq(db::now_sql_nullable()),
                    triggers::fire_count.eq(triggers::fire_count + 1),
                    triggers::updated_at.eq(db::now_sql()),
                ))
                .execute(conn)?;
            Ok(count > 0)
        })
    }

    /// List all enabled triggers.
    pub fn list_enabled(&self) -> PersistenceResult<Vec<Trigger>> {
        self.db.with(|conn| {
            let rows = triggers::table
                .filter(triggers::enabled.eq(1))
                .order(triggers::name.asc())
                .load::<TriggerRow>(conn)?;
            Ok(rows.into_iter().map(Trigger::from_row).collect())
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
        let store = TriggerStore::new(db);

        let trigger = store
            .create(
                "on-pr-open",
                "webhook_received",
                r#"{"path":"/github/pr","method":"POST"}"#,
                "code-review",
                "review the PR",
            )
            .unwrap();

        assert_eq!(trigger.name, "on-pr-open");
        assert_eq!(trigger.trigger_type, "webhook_received");
        assert!(trigger.enabled);
        assert_eq!(trigger.fire_count, 0);

        let fetched = store.get_by_name("on-pr-open").unwrap().unwrap();
        assert_eq!(fetched.team_name, "code-review");
    }

    #[test]
    fn test_list() {
        let db = test_db();
        let store = TriggerStore::new(db);

        store
            .create("alpha", "file_watch", "{}", "team-a", "")
            .unwrap();
        store
            .create("beta", "message_received", "{}", "team-b", "")
            .unwrap();

        let all = store.list().unwrap();
        assert_eq!(all.len(), 2);
        assert_eq!(all[0].name, "alpha");
        assert_eq!(all[1].name, "beta");
    }

    #[test]
    fn test_list_by_type() {
        let db = test_db();
        let store = TriggerStore::new(db);

        store
            .create("t1", "file_watch", "{}", "team-a", "")
            .unwrap();
        store
            .create("t2", "webhook_received", "{}", "team-b", "")
            .unwrap();
        store
            .create("t3", "file_watch", "{}", "team-c", "")
            .unwrap();

        let file_triggers = store.list_by_type("file_watch").unwrap();
        assert_eq!(file_triggers.len(), 2);
    }

    #[test]
    fn test_remove() {
        let db = test_db();
        let store = TriggerStore::new(db);

        store
            .create("temp", "file_watch", "{}", "team-a", "")
            .unwrap();
        assert!(store.remove("temp").unwrap());
        assert!(!store.remove("temp").unwrap());
        assert!(store.get_by_name("temp").unwrap().is_none());
    }

    #[test]
    fn test_enable_disable() {
        let db = test_db();
        let store = TriggerStore::new(db);

        store
            .create("trig", "file_watch", "{}", "team-a", "")
            .unwrap();

        store.set_enabled("trig", false).unwrap();
        let t = store.get_by_name("trig").unwrap().unwrap();
        assert!(!t.enabled);

        store.set_enabled("trig", true).unwrap();
        let t = store.get_by_name("trig").unwrap().unwrap();
        assert!(t.enabled);
    }

    #[test]
    fn test_mark_fired() {
        let db = test_db();
        let store = TriggerStore::new(db);

        store
            .create("trig", "file_watch", "{}", "team-a", "")
            .unwrap();

        store.mark_fired("trig").unwrap();
        let t = store.get_by_name("trig").unwrap().unwrap();
        assert_eq!(t.fire_count, 1);
        assert!(t.last_fired_at.is_some());

        store.mark_fired("trig").unwrap();
        let t = store.get_by_name("trig").unwrap().unwrap();
        assert_eq!(t.fire_count, 2);
    }

    #[test]
    fn test_get_nonexistent() {
        let db = test_db();
        let store = TriggerStore::new(db);
        assert!(store.get_by_name("no-such").unwrap().is_none());
    }

    #[test]
    fn test_list_enabled() {
        let db = test_db();
        let store = TriggerStore::new(db);

        store
            .create("enabled-1", "file_watch", "{}", "team-a", "")
            .unwrap();
        store
            .create("enabled-2", "webhook_received", "{}", "team-b", "")
            .unwrap();
        store
            .create("disabled-1", "file_watch", "{}", "team-c", "")
            .unwrap();
        store.set_enabled("disabled-1", false).unwrap();

        let enabled = store.list_enabled().unwrap();
        assert_eq!(enabled.len(), 2);
        let names: Vec<_> = enabled.iter().map(|t| t.name.as_str()).collect();
        assert!(names.contains(&"enabled-1"));
        assert!(names.contains(&"enabled-2"));
    }

    #[test]
    fn test_list_by_type_excludes_disabled() {
        let db = test_db();
        let store = TriggerStore::new(db);

        store
            .create("t1", "file_watch", "{}", "team-a", "")
            .unwrap();
        store
            .create("t2", "file_watch", "{}", "team-b", "")
            .unwrap();
        store.set_enabled("t2", false).unwrap();

        let triggers = store.list_by_type("file_watch").unwrap();
        assert_eq!(triggers.len(), 1);
        assert_eq!(triggers[0].name, "t1");
    }

    #[test]
    fn test_set_enabled_nonexistent() {
        let db = test_db();
        let store = TriggerStore::new(db);
        let result = store.set_enabled("no-such", false).unwrap();
        assert!(!result);
    }

    #[test]
    fn test_mark_fired_nonexistent() {
        let db = test_db();
        let store = TriggerStore::new(db);
        let result = store.mark_fired("no-such").unwrap();
        assert!(!result);
    }
}
