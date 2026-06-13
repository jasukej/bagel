use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use thiserror::Error;

const CACHE_DIR: &str = ".bagel/cache";

#[derive(Error, Debug)]
pub enum CacheError {
    #[error("Failed to access cache: {0}")]
    IoError(#[from] io::Error),
    #[error("Failed to parse cache file '{0}': {1}")]
    ParseError(String, serde_json::Error),
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct CacheEntry {
    // Hashed inputs + command + env of the last successful build
    pub hash: String,
    pub built_at: u64,
}

/**
 * Handle for the build cache.
 * Provides a clean API while files are stored per-target on the disk.
 *
 */
#[derive(Debug, Clone, Default)]
pub struct BuildCache {
    root: PathBuf,
    entries: HashMap<String, CacheEntry>,
    dirty: HashMap<String, bool>,
}

impl BuildCache {
    /**
     * Create a new cache handle for a project
     */
    pub fn new(project_root: &Path) -> Self {
        Self {
            root: project_root.to_path_buf(),
            entries: HashMap::new(),
            dirty: HashMap::new(),
        }
    }

    /**
     * Load all cached entries from disk; primarily for reporting
     */
    pub fn load_all(&mut self) -> Result<(), CacheError> {
        let cache_dir = self.cache_dir();
        if !cache_dir.exists() {
            return Ok(());
        }

        for entry in fs::read_dir(&cache_dir)? {
            let entry = entry?;
            let path = entry.path();

            if path.extension().and_then(|s| s.to_str()) == Some("json")
                && let Some(target_name) = path.file_stem().and_then(|s| s.to_str())
                && let Ok(cache_entry) = self.load_entry(&path)
            {
                self.entries.insert(target_name.to_string(), cache_entry);
            }
        }

        Ok(())
    }

    pub fn needs_rebuild(
        &mut self,
        target_name: &str,
        current_hash: &str,
    ) -> Result<bool, CacheError> {
        if let Some(entry) = self.entries.get(target_name) {
            return Ok(entry.hash != current_hash);
        }

        let path = self.entry_path(target_name);
        if path.exists() {
            let entry = self.load_entry(&path)?;
            let needs_rebuild = entry.hash != current_hash;
            self.entries.insert(target_name.to_string(), entry);

            Ok(needs_rebuild)
        } else {
            Ok(true)
        }
    }

    /**
     * Record a sucessfully, and mark the entry as dirty
     */
    pub fn record_build(&mut self, target_name: &str, hash: String) {
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_secs())
            .unwrap_or(0);

        let entry = CacheEntry {
            hash,
            built_at: now,
        };
        self.entries.insert(target_name.to_string(), entry);
        self.dirty.insert(target_name.to_string(), true);
    }

    /**
     * Flush a single target's cache to disk.
     * Each worker can call this independently without coordination
     */
    pub fn flush_target(&mut self, target_name: &str) -> Result<(), CacheError> {
        // No changes; nothing to write
        if self.dirty.get(target_name) != Some(&true) {
            return Ok(());
        }

        if let Some(entry) = self.entries.get(target_name) {
            let cache_dir = self.cache_dir();
            fs::create_dir_all(&cache_dir)?;

            let path = self.entry_path(target_name);
            let tmp_path = cache_dir.join(format!("{}.tmp", target_name));

            let content = serde_json::to_string_pretty(entry)
                .map_err(|e| CacheError::ParseError(path.display().to_string(), e))?;
            fs::write(&tmp_path, content)?;
            fs::rename(&tmp_path, path)?;

            self.dirty.insert(target_name.to_string(), false);
        }

        Ok(())
    }

    /**
     * Flush all dirty entries to disk
     */
    pub fn flush(&mut self) -> Result<(), CacheError> {
        let dirty_targets: Vec<String> = self
            .dirty
            .iter()
            .filter(|(_, is_dirty)| **is_dirty)
            .map(|(name, _)| name.clone())
            .collect();

        for target_name in dirty_targets {
            self.flush_target(&target_name)?;
        }

        Ok(())
    }

    /**
     * Get a cache entry (if loaded)
     */
    pub fn get(&self, target_name: &str) -> Option<&CacheEntry> {
        self.entries.get(target_name)
    }

    /**
     * Invalidate a target
     */
    pub fn invalidate(&mut self, target_name: &str) -> Result<(), CacheError> {
        self.entries.remove(target_name);
        self.dirty.remove(target_name);

        let path = self.entry_path(target_name);
        if path.exists() {
            fs::remove_file(&path)?;
        }
        Ok(())
    }

    /**
     * List all cached target names; primarily for debugging.
     */
    pub fn cached_targets(&self) -> Result<Vec<String>, CacheError> {
        let cache_dir = self.cache_dir();
        if !cache_dir.exists() {
            return Ok(Vec::new());
        }

        let mut targets = Vec::new();
        for entry in fs::read_dir(&cache_dir)? {
            let entry = entry?;
            let path = entry.path();
            if path.extension().and_then(|s| s.to_str()) == Some("json")
                && let Some(name) = path.file_stem().and_then(|s| s.to_str())
            {
                targets.push(name.to_string());
            }
        }

        targets.sort();
        Ok(targets)
    }

    /**
     * Clear the cache
     */
    pub fn clear(&mut self) -> Result<(), CacheError> {
        self.entries.clear();
        self.dirty.clear();

        let cache_dir = self.cache_dir();
        if cache_dir.exists() {
            fs::remove_dir_all(&cache_dir)?;
        }
        Ok(())
    }

    /**
     * Returns default cache directory
     */
    fn cache_dir(&self) -> PathBuf {
        self.root.join(CACHE_DIR)
    }

    /**
     * Formats a target dependency to the path of its cache file
     */
    fn entry_path(&self, target_name: &str) -> PathBuf {
        self.cache_dir().join(format!("{}.json", target_name))
    }

    /**
     * Returns the contents of a cached target at given path
     */
    fn load_entry(&self, path: &Path) -> Result<CacheEntry, CacheError> {
        let content = fs::read_to_string(path)?;
        serde_json::from_str(&content)
            .map_err(|e| CacheError::ParseError(path.display().to_string(), e))
    }
}

/// Reason why a target needs to be rebuilt
#[derive(Debug, Clone, PartialEq)]
pub enum RebuildReason {
    NeverBuilt,
    InputsChanged,
    CommandChanged,
    EnvChanged,
    HashMismatch,
    ForcedRebuild,
}

impl std::fmt::Display for RebuildReason {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            RebuildReason::NeverBuilt => write!(f, "never built"),
            RebuildReason::InputsChanged => write!(f, "inputs changed"),
            RebuildReason::CommandChanged => write!(f, "command changed"),
            RebuildReason::EnvChanged => write!(f, "environment changed"),
            RebuildReason::HashMismatch => write!(f, "hash mismatch"),
            RebuildReason::ForcedRebuild => write!(f, "forced rebuild"),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_dir(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("bagel_cache_test_{}", name));
        let _ = fs::remove_dir_all(&dir); // Clean up any previous test
        fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn test_cache_never_built() {
        let dir = temp_dir("never_built");
        let mut cache = BuildCache::new(&dir);

        assert!(cache.needs_rebuild("foo", "abc123").unwrap());

        fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn test_cache_hit_and_miss() {
        let dir = temp_dir("hit_miss");
        let mut cache = BuildCache::new(&dir);

        cache.record_build("foo", "abc123".to_string());
        cache.flush_target("foo").unwrap();

        // Same hash = no rebuild needed
        assert!(!cache.needs_rebuild("foo", "abc123").unwrap());

        // Different hash = rebuild needed
        assert!(cache.needs_rebuild("foo", "different").unwrap());

        // Different target = rebuild needed
        assert!(cache.needs_rebuild("bar", "abc123").unwrap());

        fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn test_cache_persistence() {
        let dir = temp_dir("persistence");

        // First run: record a build
        {
            let mut cache = BuildCache::new(&dir);
            cache.record_build("target1", "hash1".to_string());
            cache.record_build("target2", "hash2".to_string());
            cache.flush().unwrap();
        }

        // Second run: load from disk
        {
            let mut cache = BuildCache::new(&dir);
            assert!(!cache.needs_rebuild("target1", "hash1").unwrap());
            assert!(!cache.needs_rebuild("target2", "hash2").unwrap());
            assert!(cache.needs_rebuild("target1", "wrong").unwrap());
        }

        fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn test_cache_invalidate() {
        let dir = temp_dir("invalidate");
        let mut cache = BuildCache::new(&dir);

        cache.record_build("foo", "abc123".to_string());
        cache.flush_target("foo").unwrap();

        assert!(!cache.needs_rebuild("foo", "abc123").unwrap());

        cache.invalidate("foo").unwrap();

        assert!(cache.needs_rebuild("foo", "abc123").unwrap());

        fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn test_cache_clear() {
        let dir = temp_dir("clear");
        let mut cache = BuildCache::new(&dir);

        cache.record_build("a", "1".to_string());
        cache.record_build("b", "2".to_string());
        cache.flush().unwrap();

        cache.clear().unwrap();

        assert!(cache.needs_rebuild("a", "1").unwrap());
        assert!(cache.needs_rebuild("b", "2").unwrap());

        fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn test_cached_targets_list() {
        let dir = temp_dir("list");
        let mut cache = BuildCache::new(&dir);

        cache.record_build("zebra", "1".to_string());
        cache.record_build("alpha", "2".to_string());
        cache.record_build("beta", "3".to_string());
        cache.flush().unwrap();

        let targets = cache.cached_targets().unwrap();
        assert_eq!(targets, vec!["alpha", "beta", "zebra"]); // Sorted

        fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn test_parallel_safe_writes() {
        // Simulate parallel builds: two "workers" updating different targets
        let dir = temp_dir("parallel");

        // Worker 1
        let mut cache1 = BuildCache::new(&dir);
        cache1.record_build("target_a", "hash_a".to_string());

        // Worker 2
        let mut cache2 = BuildCache::new(&dir);
        cache2.record_build("target_b", "hash_b".to_string());

        // Both flush independently (no coordination needed!)
        cache1.flush_target("target_a").unwrap();
        cache2.flush_target("target_b").unwrap();

        // Verify both were written
        let mut verify = BuildCache::new(&dir);
        assert!(!verify.needs_rebuild("target_a", "hash_a").unwrap());
        assert!(!verify.needs_rebuild("target_b", "hash_b").unwrap());

        fs::remove_dir_all(&dir).ok();
    }
}
