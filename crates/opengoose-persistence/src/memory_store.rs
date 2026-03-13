//! Agent memory storage: key-value store scoped per agent.
//!
//! Allows agents to remember, recall, and forget information across sessions.

use std::sync::Arc;

use diesel::prelude::*;

use crate::db::{self, Database};
use crate::error::PersistenceResult;
use crate::models::{AgentMemoryRow, NewAgentMemory};
use crate::schema::agent_memories;

/// A stored agent memory entry.
#[derive(Debug, Clone)]
pub struct AgentMemory {
    pub id: i32,
    pub agent_name: String,
    pub key: String,
    pub value: String,
    pub created_at: String,
    pub updated_at: String,
}

impl From<AgentMemoryRow> for AgentMemory {
    fn from(row: AgentMemoryRow) -> Self {
        Self {
            id: row.id,
            agent_name: row.agent_name,
            key: row.key,
            value: row.value,
            created_at: row.created_at,
            updated_at: row.updated_at,
        }
    }
}

/// Store for agent memories (key-value, scoped by agent name).
pub struct MemoryStore {
    db: Arc<Database>,
}

impl MemoryStore {
    pub fn new(db: Arc<Database>) -> Self {
        Self { db }
    }

    /// Upsert a memory for an agent. If the key already exists, update the value.
    pub fn remember(&self, agent: &str, key: &str, value: &str) -> PersistenceResult<()> {
        self.db.with(|conn| {
            // Try update first
            let updated = diesel::update(
                agent_memories::table
                    .filter(agent_memories::agent_name.eq(agent))
                    .filter(agent_memories::key.eq(key)),
            )
            .set((
                agent_memories::value.eq(value),
                agent_memories::updated_at.eq(db::now_sql()),
            ))
            .execute(conn)?;

            if updated == 0 {
                diesel::insert_into(agent_memories::table)
                    .values(NewAgentMemory {
                        agent_name: agent,
                        key,
                        value,
                    })
                    .execute(conn)?;
            }
            Ok(())
        })
    }

    /// Recall memories for an agent, optionally filtered by keyword.
    pub fn recall(
        &self,
        agent: &str,
        keyword: Option<&str>,
    ) -> PersistenceResult<Vec<AgentMemory>> {
        self.db.with(|conn| {
            let rows = if let Some(kw) = keyword {
                let pattern = format!("%{kw}%");
                agent_memories::table
                    .filter(agent_memories::agent_name.eq(agent))
                    .filter(
                        agent_memories::key
                            .like(&pattern)
                            .or(agent_memories::value.like(&pattern)),
                    )
                    .order(agent_memories::updated_at.desc())
                    .load::<AgentMemoryRow>(conn)?
            } else {
                agent_memories::table
                    .filter(agent_memories::agent_name.eq(agent))
                    .order(agent_memories::updated_at.desc())
                    .load::<AgentMemoryRow>(conn)?
            };
            Ok(rows.into_iter().map(AgentMemory::from).collect())
        })
    }

    /// Forget a specific memory by key.
    pub fn forget(&self, agent: &str, key: &str) -> PersistenceResult<()> {
        self.db.with(|conn| {
            diesel::delete(
                agent_memories::table
                    .filter(agent_memories::agent_name.eq(agent))
                    .filter(agent_memories::key.eq(key)),
            )
            .execute(conn)?;
            Ok(())
        })
    }

    /// Get all memories for an agent (for use in prime() injection).
    pub fn all_for_agent(&self, agent: &str) -> PersistenceResult<Vec<AgentMemory>> {
        self.recall(agent, None)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_db() -> Arc<Database> {
        Arc::new(Database::open_in_memory().unwrap())
    }

    #[test]
    fn test_remember_and_recall() {
        let db = test_db();
        let store = MemoryStore::new(db);

        store.remember("agent-a", "pref", "dark mode").unwrap();
        let memories = store.recall("agent-a", None).unwrap();
        assert_eq!(memories.len(), 1);
        assert_eq!(memories[0].key, "pref");
        assert_eq!(memories[0].value, "dark mode");
    }

    #[test]
    fn test_remember_upsert() {
        let db = test_db();
        let store = MemoryStore::new(db);

        store.remember("agent-a", "key", "value1").unwrap();
        store.remember("agent-a", "key", "value2").unwrap();

        let memories = store.recall("agent-a", None).unwrap();
        assert_eq!(memories.len(), 1);
        assert_eq!(memories[0].value, "value2");
    }

    #[test]
    fn test_forget() {
        let db = test_db();
        let store = MemoryStore::new(db);

        store.remember("agent-a", "key", "value").unwrap();
        store.forget("agent-a", "key").unwrap();

        let memories = store.recall("agent-a", None).unwrap();
        assert!(memories.is_empty());
    }

    #[test]
    fn test_recall_with_keyword() {
        let db = test_db();
        let store = MemoryStore::new(db);

        store.remember("agent-a", "auth-config", "jwt").unwrap();
        store.remember("agent-a", "db-config", "postgres").unwrap();
        store.remember("agent-a", "style", "dark mode").unwrap();

        let results = store.recall("agent-a", Some("config")).unwrap();
        assert_eq!(results.len(), 2);
    }

    #[test]
    fn test_agent_isolation() {
        let db = test_db();
        let store = MemoryStore::new(db);

        store.remember("agent-a", "key", "a-value").unwrap();
        store.remember("agent-b", "key", "b-value").unwrap();

        let a_mem = store.recall("agent-a", None).unwrap();
        assert_eq!(a_mem.len(), 1);
        assert_eq!(a_mem[0].value, "a-value");

        let b_mem = store.recall("agent-b", None).unwrap();
        assert_eq!(b_mem.len(), 1);
        assert_eq!(b_mem[0].value, "b-value");
    }

    #[test]
    fn test_all_for_agent() {
        let db = test_db();
        let store = MemoryStore::new(db);

        store.remember("agent-a", "k1", "v1").unwrap();
        store.remember("agent-a", "k2", "v2").unwrap();

        let all = store.all_for_agent("agent-a").unwrap();
        assert_eq!(all.len(), 2);
    }
}
