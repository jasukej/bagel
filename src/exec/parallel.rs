use crate::exec::types::{BuildReport, ExecConfig, ExecError, TargetResult, TargetStatus};
use crate::core::BuildSpec;
use crate::utils::term::{BOLD, DIM, GREEN, RED, RESET};
use crate::utils::{BuildCache, compute_target_hash, expand_globs};
use rayon::prelude::*;
use std::collections::HashMap;
use std::process::{Command, Output, Stdio};
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Instant;

/**
 * Parallel executor; builds independent targets concurrently using rayon.
 */
pub struct ParallelExecutor {
    config: ExecConfig,
}

impl ParallelExecutor {
    pub fn new(config: ExecConfig) -> Result<Self, ExecError> {
        Ok(Self { config })
    }

    /**
     * Execute all targets in the build spec, running independent targets in parallel.
     */
    pub fn execute_all(&mut self, spec: &BuildSpec) -> Result<BuildReport, ExecError> {
        let start = Instant::now();

        // Reverse dependency map: target -> list of targets that depend on it
        let mut dependents: HashMap<&str, Vec<&str>> = HashMap::new();
        let mut remaining_deps: HashMap<&str, AtomicUsize> = HashMap::new();

        for (name, target) in &spec.targets {
            remaining_deps.insert(name.as_str(), AtomicUsize::new(target.deps.len()));
            for dep in &target.deps {
                dependents
                    .entry(dep.as_str())
                    .or_default()
                    .push(name.as_str());
            }
        }

        // Populate with no-dependency targets, which can be executed immediately
        let ready: Vec<&str> = spec
            .targets
            .iter()
            .filter(|(_, t)| t.deps.is_empty())
            .map(|(name, _)| name.as_str())
            .collect();

        // Shared state
        let results: Arc<Mutex<Vec<TargetResult>>> = Arc::new(Mutex::new(Vec::new()));
        let has_error = Arc::new(AtomicBool::new(false));
        let completed: Arc<Mutex<Vec<&str>>> = Arc::new(Mutex::new(Vec::new()));

        let mut current_wave = ready;
        let mut wave_num = 0usize;

        while !current_wave.is_empty() {
            if has_error.load(Ordering::Relaxed) && !self.config.continue_on_error {
                break;
            }

            wave_num += 1;
            let mut sorted_wave = current_wave.clone();
            sorted_wave.sort_unstable();
            println!(
                "\n{BOLD}Wave {wave_num}{RESET} — {} target(s) in parallel: {}",
                sorted_wave.len(),
                sorted_wave.join(", ")
            );

            let wave_results: Vec<TargetResult> = current_wave
                .par_iter()
                .filter_map(|&target_name| {
                    if has_error.load(Ordering::Relaxed) && !self.config.continue_on_error {
                        return None;
                    }

                    let target = spec.get_target(target_name)?;
                    let result = self.execute_target(target_name, target, start);

                    match result {
                        Ok(r) => {
                            if matches!(r.status, TargetStatus::Failed(_) | TargetStatus::Signaled)
                            {
                                has_error.store(true, Ordering::Relaxed);
                            }
                            Some(r)
                        }
                        Err(_) => {
                            has_error.store(true, Ordering::Relaxed);
                            Some(TargetResult {
                                target_name: target_name.to_string(),
                                status: TargetStatus::Failed(-1),
                                start_offset: start.elapsed(),
                                duration: std::time::Duration::ZERO,
                                output: None,
                            })
                        }
                    }
                })
                .collect();

            {
                let mut comp = completed.lock().unwrap();
                let mut res = results.lock().unwrap();
                for result in &wave_results {
                    comp.push(Box::leak(result.target_name.clone().into_boxed_str()));
                    res.push(result.clone());

                    if let Some(output) = &result.output
                        && !output.is_empty()
                    {
                        println!("  {DIM}[{}] {}{RESET}", result.target_name, output.trim());
                    }
                    match &result.status {
                        TargetStatus::Built => println!(
                            "  {GREEN}✓ {} completed in {:.2}s{RESET}",
                            result.target_name,
                            result.duration.as_secs_f64()
                        ),
                        TargetStatus::Skipped => {
                            println!("  {DIM}○ {} (up to date){RESET}", result.target_name)
                        }
                        TargetStatus::Failed(code) => println!(
                            "  {RED}✗ {} failed with exit code {}{RESET}",
                            result.target_name, code
                        ),
                        TargetStatus::Signaled => println!(
                            "  {RED}✗ {} was terminated by signal{RESET}",
                            result.target_name
                        ),
                    }
                }
            }

            let mut next_wave = Vec::new();
            for completed_target in &wave_results {
                if let Some(deps) = dependents.get(completed_target.target_name.as_str()) {
                    for &dependent in deps {
                        if let Some(counter) = remaining_deps.get(dependent) {
                            let prev = counter.fetch_sub(1, Ordering::SeqCst);
                            if prev == 1 {
                                next_wave.push(dependent);
                            }
                        }
                    }
                }
            }

            current_wave = next_wave;
        }

        let final_results = Arc::try_unwrap(results)
            .map(|mutex| mutex.into_inner().unwrap_or_default())
            .unwrap_or_else(|arc| arc.lock().unwrap().clone());

        Ok(BuildReport {
            results: final_results,
            total_duration: start.elapsed(),
        })
    }

    fn execute_target(
        &self,
        name: &str,
        target: &crate::core::TargetSpec,
        build_start: Instant,
    ) -> Result<TargetResult, ExecError> {
        let start = Instant::now();
        let start_offset = start.duration_since(build_start);

        // Designate each parallel worker its own cache handle
        let mut cache = BuildCache::new(&self.config.project_root);

        let input_files = expand_globs(&target.inputs, &self.config.project_root)?;
        let curr_hash = compute_target_hash(&input_files, &target.cmd, &target.env)?;

        let needs_rebuild =
            self.config.force_rebuild || cache.needs_rebuild(name, &curr_hash).unwrap_or(true);

        if !needs_rebuild {
            return Ok(TargetResult {
                target_name: name.to_string(),
                status: TargetStatus::Skipped,
                start_offset,
                duration: start.elapsed(),
                output: None,
            });
        }

        let output = self.run_command_captured(&target.cmd, &target.env)?;

        let result_status = if output.status.success() {
            cache.record_build(name, curr_hash);
            cache.flush_target(name)?;
            TargetStatus::Built
        } else if let Some(code) = output.status.code() {
            TargetStatus::Failed(code)
        } else {
            TargetStatus::Signaled
        };

        let duration = start.elapsed();

        // Combine stdout and stderr
        let combined_output = format!(
            "{}{}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        );

        Ok(TargetResult {
            target_name: name.to_string(),
            status: result_status,
            start_offset,
            duration,
            output: if combined_output.is_empty() {
                None
            } else {
                Some(combined_output)
            },
        })
    }

    fn run_command_captured(
        &self,
        cmd: &str,
        env: &HashMap<String, String>,
    ) -> Result<Output, ExecError> {
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

        command.stdout(Stdio::piped());
        command.stderr(Stdio::piped());

        command
            .output()
            .map_err(|e| ExecError::CommandError(cmd.to_string(), e))
    }
}
