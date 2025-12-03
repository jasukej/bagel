use bagel_core::BuildSpec;
use bagel_exec::{ExecConfig, ParallelExecutor, SerialExecutor, TargetStatus};
use std::env;
use std::path::Path;

fn main() {
    let args: Vec<String> = env::args().collect();

    let command = args.get(1).map(|s| s.as_str()).unwrap_or("build");
    let force = args.iter().any(|a| a == "--force" || a == "-f");
    let verbose = args.iter().any(|a| a == "--verbose" || a == "-v");
    let parallel = args.iter().any(|a| a == "--parallel" || a == "-j");

    match command {
        "build" => run_build(force, verbose, parallel),
        "info" => show_info(),
        "--help" | "-h" | "help" => show_help(),
        _ => {
            eprintln!("Unknown command: {}", command);
            eprintln!("Run 'bagel --help' for usage");
            std::process::exit(1);
        }
    }
}

fn show_help() {
    println!("Bagel - a simple, lightweight build system");
    println!();
    println!("USAGE:");
    println!("    bagel [COMMAND] [OPTIONS]");
    println!();
    println!("COMMANDS:");
    println!("    build    Build all targets (default)");
    println!("    info     Show build spec info without building");
    println!("    help     Show this help message");
    println!();
    println!("OPTIONS:");
    println!("    -f, --force      Force rebuild all targets (ignore cache)");
    println!("    -j, --parallel   Build targets in parallel");
    println!("    -v, --verbose    Show verbose output");
    println!("    -h, --help       Show help");
}

fn show_info() {
    let build_file = "Bagel.toml";

    if !Path::new(build_file).exists() {
        println!("No {build_file} found in current directory");
        show_getting_started();
        return;
    }

    match BuildSpec::from_file(build_file) {
        Ok(spec) => {
            if spec.targets.is_empty() {
                println!("{build_file} exists but contains no targets");
                return;
            }

            println!("Build spec: {}", build_file);
            println!("Targets: {}", spec.targets.len());
            println!();

            match spec.topological_sort() {
                Ok(order) => {
                    println!("Build order:");
                    for (i, target_name) in order.iter().enumerate() {
                        let target = spec.get_target(target_name).unwrap();
                        let deps_str = if target.deps.is_empty() {
                            "no deps".to_string()
                        } else {
                            format!("deps: {}", target.deps.join(", "))
                        };
                        println!("  {}. {} ({})", i + 1, target_name, deps_str);
                    }
                }
                Err(e) => {
                    eprintln!("Failed to compute build order: {e}");
                    std::process::exit(1);
                }
            }
        }
        Err(e) => {
            eprintln!("Failed to parse {build_file}: {e}");
            std::process::exit(1);
        }
    }
}

fn run_build(force: bool, verbose: bool, parallel: bool) {
    let build_file = "Bagel.toml";

    if !Path::new(build_file).exists() {
        eprintln!("No {build_file} found in current directory");
        show_getting_started();
        std::process::exit(1);
    }

    let spec = match BuildSpec::from_file(build_file) {
        Ok(spec) => spec,
        Err(e) => {
            eprintln!("Failed to parse {build_file}: {e}");
            std::process::exit(1);
        }
    };

    if spec.targets.is_empty() {
        println!("No targets defined in {build_file}");
        return;
    }

    let project_root = env::current_dir().expect("Failed to get current directory");

    let mut config = ExecConfig::new(project_root);
    config.force_rebuild = force;
    config.verbose = verbose;
    config.parallel = parallel;

    let mode = if parallel { "parallel" } else { "serial" };
    println!(
        "Building {} target(s) ({} mode)...",
        spec.targets.len(),
        mode
    );
    println!();

    let report = if parallel {
        let mut executor = match ParallelExecutor::new(config) {
            Ok(e) => e,
            Err(e) => {
                eprintln!("Failed to initialize executor: {e}");
                std::process::exit(1);
            }
        };

        match executor.execute_all(&spec) {
            Ok(r) => {
                for result in &r.results {
                    if let Some(output) = &result.output {
                        if !output.is_empty() {
                            println!("[{}] {}", result.target_name, output.trim());
                        }
                    }
                    match &result.status {
                        TargetStatus::Built => {
                            println!(
                                "    {} completed in {:.2}s",
                                result.target_name,
                                result.duration.as_secs_f64()
                            );
                        }
                        TargetStatus::Skipped => {
                            if verbose {
                                println!("Skipping {} (up to date)", result.target_name);
                            }
                        }
                        TargetStatus::Failed(code) => {
                            eprintln!("    {} failed with exit code {}", result.target_name, code);
                        }
                        TargetStatus::Signaled => {
                            eprintln!("    {} was terminated by signal", result.target_name);
                        }
                    }
                }
                r
            }
            Err(e) => {
                eprintln!("Build failed: {e}");
                std::process::exit(1);
            }
        }
    } else {
        let mut executor = match SerialExecutor::new(config) {
            Ok(e) => e,
            Err(e) => {
                eprintln!("Failed to initialize executor: {e}");
                std::process::exit(1);
            }
        };

        match executor.execute_all(&spec) {
            Ok(r) => r,
            Err(e) => {
                eprintln!("Build failed: {e}");
                std::process::exit(1);
            }
        }
    };

    println!();
    println!("─────────────────────────────────────");
    println!(
        "Build completed in {:.2}s",
        report.total_duration.as_secs_f64()
    );
    println!("  Built:   {}", report.built_count());
    println!("  Skipped: {}", report.skipped_count());

    if report.failed_count() > 0 {
        println!("  Failed:  {}", report.failed_count());
        println!();

        for result in &report.results {
            match &result.status {
                TargetStatus::Failed(code) => {
                    eprintln!("  - {} (exit code {})", result.target_name, code);
                }
                TargetStatus::Signaled => {
                    eprintln!("  - {} (signaled)", result.target_name);
                }
                _ => {}
            }
        }

        std::process::exit(1);
    }

    println!();
    println!("All targets built successfully!");
}

fn show_getting_started() {
    println!();
    println!("To get started, create a Bagel.toml file:");
    println!();
    println!("  [my_target]");
    println!("  cmd = \"gcc -o hello hello.c\"");
    println!("  inputs = [\"hello.c\"]");
    println!("  outputs = [\"hello\"]");
    println!();
    println!("Then run 'bagel build' to build your project.");
}
