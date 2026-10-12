use crate::core::BuildSpec;
use crate::exec::target::execute_target;
use crate::exec::types::{BuildReport, ExecConfig, ExecError, TargetResult, TargetStatus};
use crate::utils::BuildCache;
use crate::utils::term::{BOLD, DIM, GREEN, RED, RESET};
use rayon::prelude::*;
use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Instant;

/**
 * Parallel executor that builds independent targets with rayon.
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
     * This is largely just a topological sort implementation.
     */
    pub fn execute_all(&mut self, spec: &BuildSpec) -> Result<BuildReport, ExecError> {
        let start = Instant::now();

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

        let ready: Vec<&str> = spec
            .targets
            .iter()
            .filter(|(_, t)| t.deps.is_empty())
            .map(|(name, _)| name.as_str())
            .collect();

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
                    // Each worker gets its own cache handle; entries are separate files
                    let mut cache = BuildCache::new(&self.config.project_root);
                    let result =
                        execute_target(target_name, target, start, &self.config, &mut cache, true);

                    match result {
                        Ok(r) => {
                            if matches!(r.status, TargetStatus::Failed(_) | TargetStatus::Signaled)
                            {
                                has_error.store(true, Ordering::Relaxed);
                            }
                            Some(r)
                        }
                        Err(e) => {
                            eprintln!("  {RED}✗ {target_name}: {e}{RESET}");
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
                    print_result(result);
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
}

/**
 * Print a finished target's captured output followed by a one-line summary.
 */
fn print_result(result: &TargetResult) {
    let name = &result.target_name;
    if let Some(output) = &result.output {
        println!("  {DIM}[{name}] {}{RESET}", output.trim());
    }
    match result.status {
        TargetStatus::Built => println!(
            "  {GREEN}✓ {name} completed in {:.2}s{RESET}",
            result.duration.as_secs_f64()
        ),
        TargetStatus::Skipped => println!("  {DIM}○ {name} (up to date){RESET}"),
        TargetStatus::Failed(code) => {
            println!("  {RED}✗ {name} failed with exit code {code}{RESET}")
        }
        TargetStatus::Signaled => println!("  {RED}✗ {name} was terminated by signal{RESET}"),
    }
}
