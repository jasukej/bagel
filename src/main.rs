use bagel::core::BuildSpec;
use bagel::exec::{ExecConfig, ParallelExecutor, SerialExecutor, TargetStatus};
use bagel::utils::term::{BOLD, GREEN, RED, RESET};
use std::env;
use std::path::Path;

/// Parsed command-line invocation, e.g. `bagel build -j --json report.json`
struct CliArgs {
    command: String,
    force: bool,
    verbose: bool,
    parallel: bool,
    json_path: Option<String>,
    out_path: Option<String>,
}

fn parse_args(args: &[String]) -> CliArgs {
    let command = args
        .get(1)
        .map(|s| s.as_str())
        .unwrap_or("build")
        .to_string();
    let force = args.iter().any(|a| a == "--force" || a == "-f");
    let verbose = args.iter().any(|a| a == "--verbose" || a == "-v");
    let parallel = args.iter().any(|a| a == "--parallel" || a == "-j");
    let json_path = args
        .iter()
        .position(|a| a == "--json")
        .and_then(|i| args.get(i + 1))
        .cloned();
    let out_path = args
        .iter()
        .position(|a| a == "--out")
        .and_then(|i| args.get(i + 1))
        .cloned();

    CliArgs {
        command,
        force,
        verbose,
        parallel,
        json_path,
        out_path,
    }
}

fn main() {
    let args: Vec<String> = env::args().collect();
    let cli = parse_args(&args);

    match cli.command.as_str() {
        "build" => run_build(cli.force, cli.verbose, cli.parallel, cli.json_path),
        "info" => show_info(),
        "graph" => run_graph(cli.out_path),
        "--help" | "-h" | "help" => show_help(),
        _ => {
            eprintln!("Unknown command: {}", cli.command);
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
    println!("    graph    Print the dependency graph as Graphviz DOT");
    println!("    help     Show this help message");
    println!();
    println!("OPTIONS:");
    println!("    -f, --force      Force rebuild all targets (ignore cache)");
    println!("    -j, --parallel   Build targets in parallel");
    println!("    -v, --verbose    Show verbose output");
    println!("    --json <path>    Write a machine-readable JSON build report");
    println!("    --out <path>     Write 'graph' output to a file instead of stdout");
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

fn run_graph(out_path: Option<String>) {
    let build_file = "Bagel.toml";

    if !Path::new(build_file).exists() {
        eprintln!("No {build_file} found in current directory");
        std::process::exit(1);
    }

    let spec = match BuildSpec::from_file(build_file) {
        Ok(spec) => spec,
        Err(e) => {
            eprintln!("Failed to parse {build_file}: {e}");
            std::process::exit(1);
        }
    };

    let dot = spec.to_dot();

    match out_path {
        Some(path) => {
            if let Err(e) = std::fs::write(&path, &dot) {
                eprintln!("Failed to write graph to {path}: {e}");
                std::process::exit(1);
            }
            println!("Graph written to {path}");
            println!("Render it with: dot -Tpng {path} -o graph.png");
        }
        None => print!("{dot}"),
    }
}

fn run_build(force: bool, verbose: bool, parallel: bool, json_path: Option<String>) {
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
        "{BOLD}Building {} target(s) ({} mode)...{RESET}",
        spec.targets.len(),
        mode
    );

    let report = if parallel {
        let mut executor = match ParallelExecutor::new(config) {
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

    if let Some(path) = &json_path {
        let json = report.to_json(&spec);
        match serde_json::to_string_pretty(&json) {
            Ok(text) => {
                if let Err(e) = std::fs::write(path, text) {
                    eprintln!("Failed to write JSON report to {path}: {e}");
                } else {
                    println!("\nJSON report written to {path}");
                }
            }
            Err(e) => eprintln!("Failed to serialize JSON report: {e}"),
        }
    }

    println!();
    println!("─────────────────────────────────────");
    println!(
        "{BOLD}Build completed in {:.2}s{RESET}",
        report.total_duration.as_secs_f64()
    );
    println!("  {GREEN}Built:   {}{RESET}", report.built_count());
    println!("  Skipped: {}", report.skipped_count());

    if report.failed_count() > 0 {
        println!("  {RED}Failed:  {}{RESET}", report.failed_count());
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
    println!("{GREEN}{BOLD}All targets built successfully!{RESET}");
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
