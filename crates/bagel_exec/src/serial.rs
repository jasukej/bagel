use crate::types::{BuildReport, ExecConfig, ExecError, TargetResult, TargetStatus};
use bagel_core::{BuildSpec, TargetSpec};
use bagel_utils::{BuildCache, compute_target_hash, expand_globs};
use std::collections::HashMap;
use std::process::{Command, ExitStatus, Stdio};
use std::time::Instant;

/**
 * Serial executor; builds targets sequentially in topological order.
 */
pub struct SerialExecutor {
    config: ExecConfig,
    cache: BuildCache,
}

impl SerialExecutor {
    pub fn new(config: ExecConfig) -> Result<Self, ExecError> {
        let cache = BuildCache::new(&config.project_root);
        Ok(Self { config, cache })
    }

    /**
     * Execute all targets in the build spec.
     */
    pub fn execute_all(&mut self, spec: &BuildSpec) -> Result<BuildReport, ExecError> {
        let start = Instant::now();
        let order = spec.topological_sort()?;
        let mut results = Vec::new();

        for target_name in &order {
            let target = spec
                .get_target(target_name)
                .ok_or_else(|| ExecError::TargetNotFound(target_name.clone()))?;

            let result = self.execute_target(target_name, target)?;

            let failed = matches!(
                result.status,
                TargetStatus::Failed(_) | TargetStatus::Signaled
            );
            results.push(result);

            if failed && !self.config.continue_on_error {
                break;
            }
        }

        Ok(BuildReport {
            results,
            total_duration: start.elapsed(),
        })
    }

    /**
     * Execute a single target. Assumes its dependencies have been built.
     */
    fn execute_target(
        &mut self,
        name: &str,
        target: &TargetSpec,
    ) -> Result<TargetResult, ExecError> {
        let start = Instant::now();

        let input_files = expand_globs(&target.inputs, &self.config.project_root)?;
        let curr_hash = compute_target_hash(&input_files, &target.cmd, &target.env)?;

        let needs_rebuild =
            self.config.force_rebuild || self.cache.needs_rebuild(name, &curr_hash).unwrap_or(true);

        if !needs_rebuild {
            if self.config.verbose {
                println!("Skipping {} (up to date)", name);
            }
            return Ok(TargetResult {
                target_name: name.to_string(),
                status: TargetStatus::Skipped,
                duration: start.elapsed(),
                output: None,
            });
        }

        println!("Building {}...", name);
        if self.config.verbose {
            println!("   cmd: {}", target.cmd);
        }

        let status = self.run_command(&target.cmd, &target.env)?;
        let result_status = if status.success() {
            self.cache.record_build(name, curr_hash);
            self.cache.flush_target(name)?;
            TargetStatus::Built
        } else if let Some(code) = status.code() {
            TargetStatus::Failed(code)
        } else {
            TargetStatus::Signaled
        };

        let duration = start.elapsed();

        match &result_status {
            TargetStatus::Built => {
                println!("    {} completed in {:.2}s", name, duration.as_secs_f64());
            }
            TargetStatus::Failed(code) => {
                eprintln!("    {} failed with exit code {}", name, code);
            }
            TargetStatus::Signaled => {
                eprintln!("    {} was terminated by signal", name);
            }
            TargetStatus::Skipped => unreachable!(),
        }

        Ok(TargetResult {
            target_name: name.to_string(),
            status: result_status,
            duration,
            output: None,
        })
    }

    fn run_command(
        &self,
        cmd: &str,
        env: &HashMap<String, String>,
    ) -> Result<ExitStatus, ExecError> {
        let mut command = if cfg!(target_os = "windows") {
            let mut c = Command::new("cmd");
            c.args(["/C", cmd]);
            c
        } else {
            let mut c = Command::new("sh");
            c.args(["-c", cmd]);
            c
        };

        command.current_dir(&self.config.project_root);

        for (key, value) in env {
            command.env(key, value);
        }

        command.stdout(Stdio::inherit());
        command.stderr(Stdio::inherit());

        command
            .status()
            .map_err(|e| ExecError::CommandError(cmd.to_string(), e))
    }
}
