//! Utility functions for the bagel build system

pub mod cache;

use sha2::{Digest, Sha256};
use std::fs::File;
use std::io::{BufReader, Read};
use std::path::Path;
use thiserror::Error;

pub use cache::{BuildCache, CacheEntry, CacheError, RebuildReason};

#[derive(Error, Debug)]
pub enum HashError {
    #[error("Failed to read file '{0}': {1}")]
    IoError(String, std::io::Error),
    #[error("Glob pattern error: {0}")]
    GlobError(#[from] glob::PatternError),
    #[error("No files matched pattern: {0}")]
    NoFilesMatched(String),
}

/**
 * Hash a single file and return hex-encoded sha-256
 */
pub fn hash_file<P: AsRef<Path>>(path: P) -> Result<String, HashError> {
    let path = path.as_ref();
    let file = File::open(path).map_err(|e| HashError::IoError(path.display().to_string(), e))?;

    // Use a buffered reader to be efficient for large files
    let mut reader = BufReader::with_capacity(64 * 1024, file);
    let mut hasher = Sha256::new();
    let mut buffer = [0u8; 64 * 1024];

    loop {
        let bytes_read = reader
            .read(&mut buffer)
            .map_err(|e| HashError::IoError(path.display().to_string(), e))?;
        if bytes_read == 0 {
            break;
        }
        hasher.update(&buffer[..bytes_read]);
    }

    Ok(hex::encode(hasher.finalize()))
}

/**
 * Hash multiple files and combine into a single hash.
 */
pub fn hash_files<P: AsRef<Path>>(paths: &[P]) -> Result<String, HashError> {
    let mut combined_hasher = Sha256::new();

    for path in paths {
        let file_hash = hash_file(path)?;

        combined_hasher.update(path.as_ref().to_string_lossy().as_bytes());
        combined_hasher.update(b":");
        combined_hasher.update(file_hash.as_bytes());
        combined_hasher.update(b"\n");
    }

    Ok(hex::encode(combined_hasher.finalize()))
}

/**
 * Expand glob patterns and return matching file paths
 */
pub fn expand_globs(
    patterns: &[String],
    base_dir: &Path,
) -> Result<Vec<std::path::PathBuf>, HashError> {
    let mut files = Vec::new();

    for pattern in patterns {
        let full_pattern = base_dir.join(pattern);
        let pattern_str = full_pattern.to_string_lossy();

        let matches: Vec<_> = glob::glob(&pattern_str)?.filter_map(Result::ok).collect();

        if matches.is_empty() {
            if !pattern.contains('*') && !pattern.contains('?') {
                let literal_path = base_dir.join(pattern);
                if literal_path.exists() {
                    files.push(literal_path);
                } else {
                    return Err(HashError::NoFilesMatched(pattern.clone()));
                }
            } else {
                return Err(HashError::NoFilesMatched(pattern.clone()));
            }
        } else {
            files.extend(matches);
        }
    }

    files.sort();
    Ok(files)
}

/**
 * Hash a string (useful for hashing commands)
 */
pub fn hash_string(s: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(s.as_bytes());
    hex::encode(hasher.finalize())
}

/**
 * Compute a combined hash for a target's inputs and command
 * This becomes the cache key upon running change detection
 */
pub fn compute_target_hash(
    input_files: &[std::path::PathBuf],
    command: &str,
    env: &std::collections::HashMap<String, String>,
) -> Result<String, HashError> {
    let mut hasher = Sha256::new();

    for path in input_files {
        let file_hash = hash_file(path)?;
        hasher.update(path.to_string_lossy().as_bytes());
        hasher.update(b":");
        hasher.update(file_hash.as_bytes());
        hasher.update(b"\n");
    }

    hasher.update(b"cmd:");
    hasher.update(command.as_bytes());
    hasher.update(b"\n");

    let mut env_pairs: Vec<_> = env.iter().collect();
    env_pairs.sort_by_key(|(k, _)| *k);
    for (key, value) in env_pairs {
        hasher.update(b"env:");
        hasher.update(key.as_bytes());
        hasher.update(b"=");
        hasher.update(value.as_bytes());
        hasher.update(b"\n");
    }

    Ok(hex::encode(hasher.finalize()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    #[test]
    fn test_hash_string() {
        let hash1 = hash_string("hello world");
        let hash2 = hash_string("hello world");
        let hash3 = hash_string("hello world!");

        assert_eq!(hash1, hash2, "Same input should produce same hash");
        assert_ne!(
            hash1, hash3,
            "Different input should produce different hash"
        );
        assert_eq!(hash1.len(), 64, "SHA-256 hex should be 64 chars");
    }

    #[test]
    fn test_hash_file() {
        let dir = std::env::temp_dir().join("bagel_test_hash");
        std::fs::create_dir_all(&dir).unwrap();

        let file_path = dir.join("test.txt");
        let mut file = File::create(&file_path).unwrap();
        file.write_all(b"test content").unwrap();

        let hash = hash_file(&file_path).unwrap();
        assert_eq!(hash.len(), 64);

        // Same content should produce same hash
        let file_path2 = dir.join("test2.txt");
        let mut file2 = File::create(&file_path2).unwrap();
        file2.write_all(b"test content").unwrap();

        let hash2 = hash_file(&file_path2).unwrap();
        assert_eq!(hash, hash2);

        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn test_hash_files_order_matters() {
        let dir = std::env::temp_dir().join("bagel_test_hash_order");
        std::fs::create_dir_all(&dir).unwrap();

        let file_a = dir.join("a.txt");
        let file_b = dir.join("b.txt");
        std::fs::write(&file_a, "content a").unwrap();
        std::fs::write(&file_b, "content b").unwrap();

        let hash_ab = hash_files(&[&file_a, &file_b]).unwrap();
        let hash_ba = hash_files(&[&file_b, &file_a]).unwrap();

        // Order matters for reproducibility
        assert_ne!(hash_ab, hash_ba, "File order should affect hash");

        std::fs::remove_dir_all(&dir).ok();
    }
}
