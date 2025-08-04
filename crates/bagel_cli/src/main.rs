use bagel_core::BuildSpec;
use std::path::Path;

fn main() {
    println!("Bagel CLI");
    let build_file = "Bagel.toml";

    if !Path::new(build_file).exists() {
        println!("No {build_file} found in current directory");
        println!();
        println!("To get started, create a {build_file} file with your build targets:");
        println!();
        println!("Example:");
        println!("```toml");
        println!("[my_target]");
        println!("cmd = \"gcc -o hello hello.c\"");
        println!("inputs = [\"hello.c\"]");
        println!("outputs = [\"hello\"]");
        println!("```");
        println!();
        println!("See examples/hello_world/ for a complete example.");
        return;
    }

    match BuildSpec::from_file(build_file) {
        Ok(spec) => {
            if spec.targets.is_empty() {
                println!("{build_file} exists but contains no targets");
                return;
            }

            println!(
                "✓ Successfully loaded {} with {} target(s):",
                build_file,
                spec.targets.len()
            );
            for name in spec.target_names() {
                let target = spec.get_target(name).unwrap();
                println!(
                    "  • {} ({})",
                    name,
                    format!("{:?}", target.kind).to_lowercase()
                );
            }
        }
        Err(e) => {
            eprintln!("✗ Failed to parse {build_file}: {e}");
            std::process::exit(1);
        }
    }
}
