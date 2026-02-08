use std::path::Path;

use chrono::Utc;
use rusqlite::{params, Connection};

use crate::error::Result;

#[derive(Debug)]
pub struct InferenceCache {
    conn: Connection,
}

impl InferenceCache {
    pub fn open(path: &Path) -> Result<Self> {
        let conn = Connection::open(path)?;
        conn.execute_batch(
            "
            CREATE TABLE IF NOT EXISTS inference_cache (
                cache_key TEXT PRIMARY KEY,
                payload TEXT NOT NULL,
                updated_at TEXT NOT NULL
            );
            ",
        )?;

        Ok(Self { conn })
    }

    pub fn get(&self, cache_key: &str) -> Result<Option<String>> {
        let mut stmt = self
            .conn
            .prepare("SELECT payload FROM inference_cache WHERE cache_key = ?1")?;
        let mut rows = stmt.query(params![cache_key])?;
        if let Some(row) = rows.next()? {
            let payload = row.get::<_, String>(0)?;
            Ok(Some(payload))
        } else {
            Ok(None)
        }
    }

    pub fn set(&self, cache_key: &str, payload: &str) -> Result<()> {
        let ts = Utc::now().to_rfc3339();
        self.conn.execute(
            "
            INSERT INTO inference_cache (cache_key, payload, updated_at)
            VALUES (?1, ?2, ?3)
            ON CONFLICT(cache_key)
            DO UPDATE SET payload = excluded.payload, updated_at = excluded.updated_at
            ",
            params![cache_key, payload, ts],
        )?;
        Ok(())
    }
}
