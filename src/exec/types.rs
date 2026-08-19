//! Shared types for build execution

use crate::core::{BuildSpec, BuildSpecError};
use crate::utils::{CacheError, HashError};
use serde::Serialize;
use std::path::PathBuf;
use std::time::Duration;
use thiserror::Error;

/**
 * Errors that can occur during build execution
 */
#[derive(Error, Debug)]
pub enum ExecError {
    #[error("Build spec error: {0}")]
    SpecError(#[from] BuildSpecError),

    #[error("Cache error: {0}")]
    CacheError(#[from] CacheError),

    #[error("Hash error: {0}")]
    HashError(#[from] HashError),

    #[error("Target '{0}' failed with exit code {1}")]
    TargetFailed(String, i32),

    #[error("Target '{0}' was terminated by signal")]
    TargetSignaled(String),

    #[error("Failed to execute command for '{0}': {1}")]
    CommandError(String, std::io::Error),

    #[error("Target '{0}' was not found in build spec")]
    TargetNotFound(String),
}

/// Result of building a single target
#[derive(Debug, Clone)]
pub struct TargetResult {
    pub target_name: String,
    pub status: TargetStatus,
    /// Offset from the start of the overall build at which this target began executing.
    /// Overlapping offsets/durations across targets are what make parallelism visible.
    pub start_offset: Duration,
    pub duration: Duration,
    pub output: Option<String>,
}

/// Status of a target build
#[derive(Debug, Clone, PartialEq)]
pub enum TargetStatus {
    Built,       // Target was built successfully
    Skipped,     // Target was skipped (already up to date)
    Failed(i32), // Target failed with given exit code
    Signaled,    // Target was terminated by signal
}

/// Represents successful/unsuccessful targets and their status
#[derive(Debug, Clone)]
pub struct BuildReport {
    pub results: Vec<TargetResult>,
    pub total_duration: Duration,
}

impl BuildReport {
    pub fn built_count(&self) -> usize {
        self.results
            .iter()
            .filter(|r| r.status == TargetStatus::Built)
            .count()
    }

    pub fn skipped_count(&self) -> usize {
        self.results
            .iter()
            .filter(|r| r.status == TargetStatus::Skipped)
            .count()
    }

    pub fn failed_count(&self) -> usize {
        self.results
            .iter()
            .filter(|r| matches!(r.status, TargetStatus::Failed(_) | TargetStatus::Signaled))
            .count()
    }

    pub fn success(&self) -> bool {
        self.failed_count() == 0
    }

    /**
     * Render the report as JSON, supplemented with each target's declared 
     * dependencies so that it can be rendered as a DAG.
     */
    pub fn to_json(&self, spec: &BuildSpec) -> JsonReport {
        let targets = self
            .results
            .iter()
            .map(|r| {
                let deps = spec
                    .get_target(&r.target_name)
                    .map(|t| t.deps.clone())
                    .unwrap_or_default();

                let (status, exit_code) = match &r.status {
                    TargetStatus::Built => ("built", None),
                    TargetStatus::Skipped => ("skipped", None),
                    TargetStatus::Failed(code) => ("failed", Some(*code)),
                    TargetStatus::Signaled => ("signaled", None),
                };

                JsonTargetResult {
                    name: r.target_name.clone(),
                    status: status.to_string(),
                    exit_code,
                    start_ms: r.start_offset.as_secs_f64() * 1000.0,
                    duration_ms: r.duration.as_secs_f64() * 1000.0,
                    deps,
                }
            })
            .collect();

        JsonReport {
            total_duration_ms: self.total_duration.as_secs_f64() * 1000.0,
            built: self.built_count(),
            skipped: self.skipped_count(),
            failed: self.failed_count(),
            targets,
        }
    }
}

/// JSON-serializable view of a [`BuildReport`] for visualization.
#[derive(Debug, Clone, Serialize)]
pub struct JsonReport {
    pub total_duration_ms: f64,
    pub built: usize,
    pub skipped: usize,
    pub failed: usize,
    pub targets: Vec<JsonTargetResult>,
}

#[derive(Debug, Clone, Serialize)]
pub struct JsonTargetResult {
    pub name: String,
    pub status: String,
    pub exit_code: Option<i32>,
    pub start_ms: f64,
    pub duration_ms: f64,
    pub deps: Vec<String>,
}

/// Configuration for build execution
#[derive(Debug, Clone)]
pub struct ExecConfig {
    pub project_root: PathBuf, // project root directory (where Bagel.toml lives)
    pub force_rebuild: bool,   // force rebuild all targets, ignoring cache
    pub continue_on_error: bool, // continue execution after a target fails to build
    pub verbose: bool,         // verbose output
    pub parallel: bool,        // execute in parallel
}

impl ExecConfig {
    pub fn new(project_root: impl Into<PathBuf>) -> Self {
        Self {
            project_root: project_root.into(),
            force_rebuild: false,
            continue_on_error: false,
            verbose: false,
            parallel: false,
        }
    }
}
