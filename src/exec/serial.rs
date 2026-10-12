use crate::core::BuildSpec;
use crate::exec::target::execute_target;
use crate::exec::types::{BuildReport, ExecConfig, ExecError, TargetResult, TargetStatus};
use crate::utils::BuildCache;
use crate::utils::term::{DIM, GREEN, RED, RESET};
use std::time::Instant;

/**
 * Serial executor that builds targets sequentially in topological order.
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

            let result = execute_target(
                target_name,
                target,
                start,
                &self.config,
                &mut self.cache,
                false,
            )?;
            self.print_result(&result, &target.cmd);

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
     * Print a one-line summary of a finished target. The command's own output
     * has already streamed to the terminal.
     */
    fn print_result(&self, result: &TargetResult, cmd: &str) {
        let name = &result.target_name;
        if self.config.verbose && result.status != TargetStatus::Skipped {
            println!("   cmd: {cmd}");
        }
        match result.status {
            TargetStatus::Built => println!(
                "{GREEN}✓ {name} completed in {:.2}s{RESET}",
                result.duration.as_secs_f64()
            ),
            TargetStatus::Skipped => println!("{DIM}○ {name} (up to date){RESET}"),
            TargetStatus::Failed(code) => {
                eprintln!("{RED}✗ {name} failed with exit code {code}{RESET}")
            }
            TargetStatus::Signaled => eprintln!("{RED}✗ {name} was terminated by signal{RESET}"),
        }
    }
}
