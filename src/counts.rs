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
    /// The lock is held until the file is written, so concurrent increments
    /// persist one at a time and the newest counts always land last
    /// (SPEC D6). A persistence error is printed to stderr only after the
    /// lock is released, and the in-memory count is kept either way.
    pub fn increment(&self, key: &str) {
        let result = {
            let mut map = self.counts.lock().unwrap();
            *map.entry(key.to_owned()).or_insert(0) += 1;
            // Persist before the guard drops: every write goes through the same
            // `.tmp` file, so two writers outside the lock would share it.
            self.persist(&map)
        };
        // Report after the guard drops: `eprintln!` panics if writing to stderr
        // fails, and a panic under the lock would poison it for every later play.
        if let Err(e) = result {
            eprintln!("counts: persist error ({}): {e}", self.file.display());
        }
    }

    /// Returns a snapshot of all current listen counts.
    pub fn snapshot(&self) -> HashMap<String, u64> {
        self.counts.lock().unwrap().clone()
    }

    /// Writes `counts` to the backing JSON file atomically (write to a
    /// `.tmp` file, then rename it over the target).
    ///
    /// Callers must hold the store's lock: the `.tmp` path is fixed, so two
    /// calls at once would race on it (see [`ListenStore::increment`]).
    /// The I/O error, if any, is returned for the caller to report once the
    /// lock is released; the in-memory state is unaffected.
    fn persist(&self, counts: &HashMap<String, u64>) -> std::io::Result<()> {
        let json = serde_json::to_string(counts).expect("counts serialisation is infallible");
        let tmp = self.file.with_extension("tmp");
        std::fs::write(&tmp, &json).and_then(|()| std::fs::rename(&tmp, &self.file))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicU64, Ordering};

    static COUNTER: AtomicU64 = AtomicU64::new(0);

    /// RAII guard that deletes the wrapped path (and its `.tmp` sibling) on drop.
    struct TempFile(PathBuf);

    impl Drop for TempFile {
        fn drop(&mut self) {
            let _ = std::fs::remove_file(&self.0);
            let _ = std::fs::remove_file(self.0.with_extension("tmp"));
        }
    }

    /// Returns a unique temp path and a [`TempFile`] guard that cleans it up on drop.
    fn tmp_file() -> (PathBuf, TempFile) {
        let n = COUNTER.fetch_add(1, Ordering::Relaxed);
        let path = std::env::temp_dir().join(format!("podserv_counts_test_{n}.json"));
        let guard = TempFile(path.clone());
        (path, guard)
    }

    // --- ListenStore::load ---

    #[test]
    fn load_missing_file_gives_empty() {
        let (path, _f) = tmp_file();
        let _ = std::fs::remove_file(&path);
        let store = ListenStore::load(path);
        assert!(store.snapshot().is_empty());
    }

    #[test]
    fn load_invalid_json_gives_empty() {
        let (path, _f) = tmp_file();
        std::fs::write(&path, "not json").unwrap();
        let store = ListenStore::load(path);
        assert!(store.snapshot().is_empty());
    }

    #[test]
    fn load_valid_file_restores_counts() {
        let (path, _f) = tmp_file();
        std::fs::write(&path, r#"{"ep.mp3":42}"#).unwrap();
        let store = ListenStore::load(path);
        assert_eq!(store.snapshot()["ep.mp3"], 42);
    }

    // --- ListenStore::increment ---

    #[test]
    fn increment_starts_at_one() {
        let (path, _f) = tmp_file();
        let store = ListenStore::load(path);
        store.increment("ep.mp3");
        assert_eq!(store.snapshot()["ep.mp3"], 1);
    }

    #[test]
    fn increment_accumulates() {
        let (path, _f) = tmp_file();
        let store = ListenStore::load(path);
        store.increment("ep.mp3");
        store.increment("ep.mp3");
        store.increment("ep.mp3");
        assert_eq!(store.snapshot()["ep.mp3"], 3);
    }

    #[test]
    fn increment_independent_keys() {
        let (path, _f) = tmp_file();
        let store = ListenStore::load(path);
        store.increment("a.mp3");
        store.increment("b.mp3");
        store.increment("a.mp3");
        let snap = store.snapshot();
        assert_eq!(snap["a.mp3"], 2);
        assert_eq!(snap["b.mp3"], 1);
    }

    // --- ListenStore::persist (via increment + reload) ---

    #[test]
    fn persist_and_reload() {
        let (path, _f) = tmp_file();
        {
            let store = ListenStore::load(path.clone());
            store.increment("ep.mp3");
            store.increment("ep.mp3");
        }
        let store2 = ListenStore::load(path);
        assert_eq!(store2.snapshot()["ep.mp3"], 2);
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

    // --- concurrent increments (SPEC D6, ACCEPTANCE § D-R10) ---

    /// Overlapping increments must leave the file complete and current.
    ///
    /// Eight threads start together and each records 100 first plays on keys
    /// no other thread uses, so every snapshot is longer than the one before
    /// it. After each `increment` returns, the file must parse and already
    /// hold that play; once all threads finish, reloading it must give the
    /// in-memory counts exactly. Writing outside the lock broke the first two:
    /// threads shared one `.tmp` file, so renames failed with ENOENT, and the
    /// file read back after an increment could be empty, torn or lack the play.
    #[test]
    fn concurrent_increments_keep_file_complete_and_current() {
        const THREADS: usize = 8;
        const PLAYS: usize = 100;
        let (path, _f) = tmp_file();
        let store = ListenStore::load(path.clone());
        let start = std::sync::Barrier::new(THREADS);
        std::thread::scope(|s| {
            for t in 0..THREADS {
                let (store, start, path) = (&store, &start, &path);
                s.spawn(move || {
                    start.wait();
                    for i in 0..PLAYS {
                        let key = format!("t{t}e{i}.mp3");
                        store.increment(&key);
                        let raw = std::fs::read_to_string(path).expect("listens file is readable");
                        let on_disk: HashMap<String, u64> =
                            serde_json::from_str(&raw).expect("listens file is valid JSON");
                        assert_eq!(
                            on_disk.get(&key),
                            Some(&1),
                            "file lacks {key} after its increment returned"
                        );
                    }
                });
            }
        });
        let reloaded = ListenStore::load(path).snapshot();
        assert_eq!(reloaded.len(), THREADS * PLAYS);
        assert_eq!(reloaded, store.snapshot());
    }

    // --- ListenStore::snapshot ---

    #[test]
    fn snapshot_returns_all_entries() {
        let (path, _f) = tmp_file();
        let store = ListenStore::load(path);
        store.increment("a.mp3");
        store.increment("b.mp3");
        let snap = store.snapshot();
        assert_eq!(snap.len(), 2);
        assert!(snap.contains_key("a.mp3"));
        assert!(snap.contains_key("b.mp3"));
    }
}
