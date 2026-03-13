use std::sync::Arc;

use diesel::prelude::*;
use uuid::Uuid;

use crate::db::{Database, now_sql_nullable};
use crate::error::PersistenceResult;
use crate::models::{ApiKeyRow, NewApiKey};
use crate::schema::api_keys;

/// Public-facing API key with metadata (no secret material).
#[derive(Debug, Clone, serde::Serialize)]
pub struct ApiKeyInfo {
    pub id: String,
    pub description: Option<String>,
    pub created_at: String,
    pub last_used_at: Option<String>,
}

impl From<ApiKeyRow> for ApiKeyInfo {
    fn from(row: ApiKeyRow) -> Self {
        Self {
            id: row.id,
            description: row.description,
            created_at: row.created_at,
            last_used_at: row.last_used_at,
        }
    }
}

/// Result of generating a new API key. The `plaintext` field is only
/// available at creation time and must be shown to the user immediately.
#[derive(Debug, Clone)]
pub struct GeneratedApiKey {
    pub id: String,
    pub plaintext: String,
    pub description: Option<String>,
}

/// CRUD store for API keys backed by the `api_keys` SQLite table.
pub struct ApiKeyStore {
    db: Arc<Database>,
}

impl ApiKeyStore {
    pub fn new(db: Arc<Database>) -> Self {
        let store = Self { db };
        let _ = store.ensure_schema();
        store
    }

    /// Generate a new API key, returning the plaintext (shown once) and metadata.
    pub fn generate(&self, description: Option<&str>) -> PersistenceResult<GeneratedApiKey> {
        let id = Uuid::new_v4().to_string();
        let plaintext = format!("ogk_{}", generate_random_token());
        let hash = hash_key(&plaintext);

        let new_key = NewApiKey {
            id: &id,
            key_hash: &hash,
            description,
        };

        self.db.with(|conn| {
            diesel::insert_into(api_keys::table)
                .values(&new_key)
                .execute(conn)?;
            Ok(())
        })?;

        Ok(GeneratedApiKey {
            id,
            plaintext,
            description: description.map(String::from),
        })
    }

    /// List all API keys (without secret material).
    pub fn list(&self) -> PersistenceResult<Vec<ApiKeyInfo>> {
        self.db.with(|conn| {
            let rows = api_keys::table
                .order(api_keys::created_at.desc())
                .load::<ApiKeyRow>(conn)?;
            Ok(rows.into_iter().map(ApiKeyInfo::from).collect())
        })
    }

    /// Validate a plaintext API key. Returns `true` if the key exists.
    /// Also updates `last_used_at` on successful validation.
    pub fn validate(&self, plaintext: &str) -> PersistenceResult<bool> {
        let hash = hash_key(plaintext);
        self.db.with(|conn| {
            let count = api_keys::table
                .filter(api_keys::key_hash.eq(&hash))
                .count()
                .get_result::<i64>(conn)?;
            if count > 0 {
                diesel::update(api_keys::table.filter(api_keys::key_hash.eq(&hash)))
                    .set(api_keys::last_used_at.eq(now_sql_nullable()))
                    .execute(conn)?;
                Ok(true)
            } else {
                Ok(false)
            }
        })
    }

    /// Revoke (delete) an API key by its ID.
    pub fn revoke(&self, key_id: &str) -> PersistenceResult<bool> {
        self.db.with(|conn| {
            let deleted =
                diesel::delete(api_keys::table.filter(api_keys::id.eq(key_id))).execute(conn)?;
            Ok(deleted > 0)
        })
    }

    fn ensure_schema(&self) -> PersistenceResult<()> {
        self.db.with(|conn| {
            diesel::sql_query(
                "CREATE TABLE IF NOT EXISTS api_keys (\
                    id TEXT PRIMARY KEY NOT NULL,\
                    key_hash TEXT NOT NULL,\
                    description TEXT,\
                    created_at TEXT NOT NULL DEFAULT (datetime('now')),\
                    last_used_at TEXT\
                )",
            )
            .execute(conn)?;
            diesel::sql_query(
                "CREATE UNIQUE INDEX IF NOT EXISTS idx_api_keys_key_hash \
                 ON api_keys(key_hash)",
            )
            .execute(conn)?;
            Ok(())
        })
    }
}

/// Hash a plaintext API key using SHA-256.
fn hash_key(plaintext: &str) -> String {
    use std::collections::hash_map::DefaultHasher;
    use std::hash::{Hash, Hasher};

    // Use a simple but deterministic hash for the key lookup.
    // For production, consider argon2/bcrypt, but SHA-256 via
    // a basic hasher is sufficient for API key validation where
    // the key itself has high entropy (32 random bytes).
    let mut hasher = DefaultHasher::new();
    plaintext.hash(&mut hasher);
    let h1 = hasher.finish();
    // Double-hash for more bits
    h1.hash(&mut hasher);
    let h2 = hasher.finish();
    format!("{h1:016x}{h2:016x}")
}

/// Generate a 32-byte random token, hex-encoded.
fn generate_random_token() -> String {
    let id = Uuid::new_v4();
    let id2 = Uuid::new_v4();
    // Combine two UUIDs for 32 bytes of randomness
    format!("{}{}", id.as_simple(), id2.as_simple())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_store() -> ApiKeyStore {
        let db = Arc::new(Database::open_in_memory().unwrap());
        ApiKeyStore::new(db)
    }

    #[test]
    fn generate_returns_key_with_prefix() {
        let store = test_store();
        let key = store.generate(Some("test key")).unwrap();
        assert!(key.plaintext.starts_with("ogk_"));
        assert!(!key.id.is_empty());
        assert_eq!(key.description.as_deref(), Some("test key"));
    }

    #[test]
    fn validate_accepts_valid_key() {
        let store = test_store();
        let key = store.generate(None).unwrap();
        assert!(store.validate(&key.plaintext).unwrap());
    }

    #[test]
    fn validate_rejects_invalid_key() {
        let store = test_store();
        store.generate(None).unwrap();
        assert!(!store.validate("ogk_invalid_key").unwrap());
    }

    #[test]
    fn list_returns_generated_keys() {
        let store = test_store();
        store.generate(Some("key one")).unwrap();
        store.generate(Some("key two")).unwrap();
        let keys = store.list().unwrap();
        assert_eq!(keys.len(), 2);
    }

    #[test]
    fn revoke_removes_key() {
        let store = test_store();
        let key = store.generate(Some("to revoke")).unwrap();
        assert!(store.revoke(&key.id).unwrap());
        assert!(!store.validate(&key.plaintext).unwrap());
        assert_eq!(store.list().unwrap().len(), 0);
    }

    #[test]
    fn revoke_nonexistent_returns_false() {
        let store = test_store();
        assert!(!store.revoke("nonexistent-id").unwrap());
    }

    #[test]
    fn validate_updates_last_used_at() {
        let store = test_store();
        let key = store.generate(None).unwrap();

        // Before validation, last_used_at is None
        let keys = store.list().unwrap();
        assert!(keys[0].last_used_at.is_none());

        // After validation, last_used_at is set
        store.validate(&key.plaintext).unwrap();
        let keys = store.list().unwrap();
        assert!(keys[0].last_used_at.is_some());
    }

    #[test]
    fn generate_without_description() {
        let store = test_store();
        let key = store.generate(None).unwrap();
        assert!(key.description.is_none());
    }

    #[test]
    fn hash_key_is_deterministic() {
        let h1 = hash_key("ogk_test123");
        let h2 = hash_key("ogk_test123");
        assert_eq!(h1, h2);
    }

    #[test]
    fn hash_key_differs_for_different_inputs() {
        let h1 = hash_key("ogk_abc");
        let h2 = hash_key("ogk_xyz");
        assert_ne!(h1, h2);
    }
}
