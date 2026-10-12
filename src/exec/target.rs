use crate::core::TargetSpec;
use crate::exec::types::{ExecConfig, ExecError, TargetResult, TargetStatus};
use crate::utils::{BuildCache, compute_target_hash, expand_globs, find_missing_output};
use std::collections::HashMap;
use std::path::Path;
use std::process::{Command, Output, Stdio};
use std::time::Instant;

/**
 * Execute a single target, assuming its dependencies have been built.
 */
pub(super) fn execute_target(
    name: &str,
    target: &TargetSpec,
    build_start: Instant,
    config: &ExecConfig,
    cache: &mut BuildCache,
    capture: bool,
) -> Result<TargetResult, ExecError> {
    let start = Instant::now();
    let start_offset = start.duration_since(build_start);

    let input_files = expand_globs(&target.inputs, &config.project_root)?;
    let curr_hash = compute_target_hash(&input_files, &target.cmd, &target.env)?;

    let needs_rebuild = config.force_rebuild
        || find_missing_output(&target.outputs, &config.project_root).is_some()
        || cache.needs_rebuild(name, &curr_hash).unwrap_or(true);

    if !needs_rebuild {
        return Ok(TargetResult {
            target_name: name.to_string(),
            status: TargetStatus::Skipped,
            start_offset,
            duration: start.elapsed(),
            output: None,
        });
    }

    let output = run_command(&target.cmd, &target.env, &config.project_root, capture)?;
    let status = if output.status.success() {
        // Don't cache a target that "succeeded" without producing its outputs
        if let Some(path) = find_missing_output(&target.outputs, &config.project_root) {
            return Err(ExecError::MissingOutput(name.to_string(), path));
        }
        cache.record_build(name, curr_hash);
        cache.flush_target(name)?;
        TargetStatus::Built
    } else if let Some(code) = output.status.code() {
        TargetStatus::Failed(code)
    } else {
        TargetStatus::Signaled
    };

    let combined_output = format!(
        "{}{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );

    Ok(TargetResult {
        target_name: name.to_string(),
        status,
        start_offset,
        duration: start.elapsed(),
        output: Some(combined_output).filter(|s| !s.is_empty()),
    })
}

/**
 * Run a shell command. When `capture` is false, stream otuput to the terminal
 * and null out the Output's stderr/stdout.
 */
fn run_command(
    cmd: &str,
    env: &HashMap<String, String>,
    cwd: &Path,
    capture: bool,
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

    command.current_dir(cwd).envs(env);
    if !capture {
        command.stdout(Stdio::inherit()).stderr(Stdio::inherit());
    }

    command
        .output()
        .map_err(|e| ExecError::CommandError(cmd.to_string(), e))
}
