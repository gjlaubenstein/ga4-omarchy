use std::{fs, path::PathBuf, time::Duration};

use chrono::{DateTime, Utc};
use serde::{de::DeserializeOwned, Deserialize, Serialize};
use sha2::{Digest, Sha256};

use crate::{
    config::Paths,
    error::{AppError, Result},
};

pub const CACHE_SCHEMA: u16 = 1;
pub const DEFAULT_TTL: Duration = Duration::from_secs(15 * 60);

#[derive(Debug, Clone)]
pub struct CacheHit<T> {
    pub value: T,
    pub fetched_at: DateTime<Utc>,
    pub fresh: bool,
}

#[derive(Serialize, Deserialize)]
struct Envelope<T> {
    schema: u16,
    fetched_at: DateTime<Utc>,
    value: T,
}

#[derive(Clone)]
pub struct CacheStore {
    root: PathBuf,
}

impl CacheStore {
    pub fn new(paths: &Paths) -> Self {
        Self {
            root: paths.cache_dir.clone(),
        }
    }

    pub fn key(parts: &[&str]) -> String {
        let mut hasher = Sha256::new();
        for part in parts {
            hasher.update(part.as_bytes());
            hasher.update([0]);
        }
        hex::encode(hasher.finalize())
    }

    pub fn get<T: DeserializeOwned>(&self, key: &str, ttl: Duration) -> Option<CacheHit<T>> {
        let data = fs::read(self.path(key)).ok()?;
        let envelope: Envelope<T> = serde_json::from_slice(&data).ok()?;
        if envelope.schema != CACHE_SCHEMA {
            return None;
        }
        let age = Utc::now()
            .signed_duration_since(envelope.fetched_at)
            .to_std()
            .unwrap_or_default();
        Some(CacheHit {
            value: envelope.value,
            fetched_at: envelope.fetched_at,
            fresh: age <= ttl,
        })
    }

    pub fn put<T: Serialize>(&self, key: &str, fetched_at: DateTime<Utc>, value: &T) -> Result<()> {
        fs::create_dir_all(&self.root).map_err(|error| AppError::Other(error.to_string()))?;
        let destination = self.path(key);
        let temporary = destination.with_extension("json.tmp");
        let bytes = serde_json::to_vec(&Envelope {
            schema: CACHE_SCHEMA,
            fetched_at,
            value,
        })
        .map_err(|error| AppError::Other(error.to_string()))?;
        fs::write(&temporary, bytes).map_err(|error| AppError::Other(error.to_string()))?;
        fs::rename(temporary, destination).map_err(|error| AppError::Other(error.to_string()))
    }

    pub fn clear(&self) -> Result<()> {
        if self.root.exists() {
            fs::remove_dir_all(&self.root).map_err(|error| AppError::Other(error.to_string()))?;
        }
        Ok(())
    }

    fn path(&self, key: &str) -> PathBuf {
        self.root.join(format!("{key}.json"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Duration as ChronoDuration;
    use tempfile::tempdir;

    #[test]
    fn cache_round_trip_and_staleness() {
        let root = tempdir().unwrap();
        let paths = Paths::from_dirs(root.path().join("config"), root.path().join("cache"));
        let cache = CacheStore::new(&paths);
        cache
            .put(
                "report",
                Utc::now() - ChronoDuration::minutes(20),
                &vec![1, 2],
            )
            .unwrap();
        let hit = cache.get::<Vec<i32>>("report", DEFAULT_TTL).unwrap();
        assert_eq!(hit.value, vec![1, 2]);
        assert!(!hit.fresh);
    }

    #[test]
    fn corrupt_cache_is_ignored() {
        let root = tempdir().unwrap();
        let paths = Paths::from_dirs(root.path().join("config"), root.path().join("cache"));
        let cache = CacheStore::new(&paths);
        fs::create_dir_all(&paths.cache_dir).unwrap();
        fs::write(paths.cache_dir.join("bad.json"), b"not-json").unwrap();
        assert!(cache.get::<serde_json::Value>("bad", DEFAULT_TTL).is_none());
    }
}
