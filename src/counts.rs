// Copyright (C) 2026 l5yth
//
// Licensed under the Apache License, Version 2.0 (the "License");
// you may not use this file except in compliance with the License.
// You may obtain a copy of the License at
//
//     http://www.apache.org/licenses/LICENSE-2.0
//
// Unless required by applicable law or agreed to in writing, software
// distributed under the License is distributed on an "AS IS" BASIS,
// WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
// See the License for the specific language governing permissions and
// limitations under the License.

//! Persistent listen-count store.
//!
//! [`ListenStore`] keeps an in-memory counter for each episode path and
//! writes the full map to a JSON file on every update, so counts survive
//! server restarts.

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Mutex;

/// In-memory listen-count store backed by a JSON file.
///
/// Counts are keyed by episode relative path (e.g. `"shows/ep1.mp3"`),
/// matching [`crate::media::Episode::rel_path`]. The store is safe to
/// share across threads and is accessed via
/// [`actix_web::web::Data`] (which wraps it in an `Arc`).
pub struct ListenStore {
    /// Live counts, protected for concurrent access.
    counts: Mutex<HashMap<String, u64>>,
    /// Path of the JSON file that backs this store.
    file: PathBuf,
}

impl ListenStore {
    /// Loads a [`ListenStore`] from the JSON file at `path`.
    ///
    /// Returns an empty store if the file is absent or cannot be parsed as
    /// a JSON object of string keys and integer values.
    pub fn load(path: PathBuf) -> Self {
        let counts = std::fs::read_to_string(&path)
            .ok()
            .and_then(|s| serde_json::from_str::<HashMap<String, u64>>(&s).ok())
            .unwrap_or_default();
        Self {
            counts: Mutex::new(counts),
            file: path,
        }
    }

    /// Increments the listen count for `key` by one and persists the
    /// updated map to disk.
    ///
    /// Persistence errors are printed to stderr but do not affect the
    /// in-memory count or the server's request handling.
    pub fn increment(&self, key: &str) {
        let snapshot = {
            let mut map = self.counts.lock().unwrap();
            *map.entry(key.to_owned()).or_insert(0) += 1;
            map.clone()
        };
        self.persist(&snapshot);
    }

    /// Returns a snapshot of all current listen counts.
    pub fn snapshot(&self) -> HashMap<String, u64> {
        self.counts.lock().unwrap().clone()
    }

    /// Writes `counts` to the backing JSON file atomically (write to a
    /// `.tmp` file, then rename it over the target).
    ///
    /// Any I/O error is logged to stderr; the in-memory state is unaffected.
    fn persist(&self, counts: &HashMap<String, u64>) {
        let json = serde_json::to_string(counts).expect("counts serialisation is infallible");
        let tmp = self.file.with_extension("tmp");
        let result = std::fs::write(&tmp, &json).and_then(|()| std::fs::rename(&tmp, &self.file));
        if let Err(e) = result {
            eprintln!("counts: persist error ({}): {e}", self.file.display());
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicU64, Ordering};

    static COUNTER: AtomicU64 = AtomicU64::new(0);

    fn tmp_path() -> PathBuf {
        let n = COUNTER.fetch_add(1, Ordering::Relaxed);
        std::env::temp_dir().join(format!("podserv_counts_test_{n}.json"))
    }

    // --- ListenStore::load ---

    #[test]
    fn load_missing_file_gives_empty() {
        let path = tmp_path();
        let _ = std::fs::remove_file(&path);
        let store = ListenStore::load(path);
        assert!(store.snapshot().is_empty());
    }

    #[test]
    fn load_invalid_json_gives_empty() {
        let path = tmp_path();
        std::fs::write(&path, "not json").unwrap();
        let store = ListenStore::load(path.clone());
        assert!(store.snapshot().is_empty());
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn load_valid_file_restores_counts() {
        let path = tmp_path();
        std::fs::write(&path, r#"{"ep.mp3":42}"#).unwrap();
        let store = ListenStore::load(path.clone());
        assert_eq!(store.snapshot()["ep.mp3"], 42);
        let _ = std::fs::remove_file(&path);
    }

    // --- ListenStore::increment ---

    #[test]
    fn increment_starts_at_one() {
        let path = tmp_path();
        let store = ListenStore::load(path.clone());
        store.increment("ep.mp3");
        assert_eq!(store.snapshot()["ep.mp3"], 1);
        let _ = std::fs::remove_file(&path);
        let _ = std::fs::remove_file(path.with_extension("tmp"));
    }

    #[test]
    fn increment_accumulates() {
        let path = tmp_path();
        let store = ListenStore::load(path.clone());
        store.increment("ep.mp3");
        store.increment("ep.mp3");
        store.increment("ep.mp3");
        assert_eq!(store.snapshot()["ep.mp3"], 3);
        let _ = std::fs::remove_file(&path);
        let _ = std::fs::remove_file(path.with_extension("tmp"));
    }

    #[test]
    fn increment_independent_keys() {
        let path = tmp_path();
        let store = ListenStore::load(path.clone());
        store.increment("a.mp3");
        store.increment("b.mp3");
        store.increment("a.mp3");
        let snap = store.snapshot();
        assert_eq!(snap["a.mp3"], 2);
        assert_eq!(snap["b.mp3"], 1);
        let _ = std::fs::remove_file(&path);
        let _ = std::fs::remove_file(path.with_extension("tmp"));
    }

    // --- ListenStore::persist (via increment + reload) ---

    #[test]
    fn persist_and_reload() {
        let path = tmp_path();
        {
            let store = ListenStore::load(path.clone());
            store.increment("ep.mp3");
            store.increment("ep.mp3");
        }
        let store2 = ListenStore::load(path.clone());
        assert_eq!(store2.snapshot()["ep.mp3"], 2);
        let _ = std::fs::remove_file(&path);
        let _ = std::fs::remove_file(path.with_extension("tmp"));
    }

    #[test]
    fn persist_failure_does_not_panic() {
        // Path in a non-existent directory → write fails gracefully.
        let path = PathBuf::from("/nonexistent_podserv_b_test_dir/counts.json");
        let store = ListenStore::load(path);
        // Must not panic; in-memory count is still incremented.
        store.increment("ep.mp3");
        assert_eq!(store.snapshot()["ep.mp3"], 1);
    }

    // --- ListenStore::snapshot ---

    #[test]
    fn snapshot_returns_all_entries() {
        let path = tmp_path();
        let store = ListenStore::load(path.clone());
        store.increment("a.mp3");
        store.increment("b.mp3");
        let snap = store.snapshot();
        assert_eq!(snap.len(), 2);
        assert!(snap.contains_key("a.mp3"));
        assert!(snap.contains_key("b.mp3"));
        let _ = std::fs::remove_file(&path);
        let _ = std::fs::remove_file(path.with_extension("tmp"));
    }
}
