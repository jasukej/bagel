Bagel is a lightweight, parallelizable build system written in Rust and C++ that:
* Builds a dependency DAG and schedules commands to run in topological order
* Allows incremental builds by skipping up-to-date targets with hashing
* Supports parallel execution of independent build steps
* Is inspired by Bazel (Google's build system)

## Introduction
We narrow down 3 key dimensions of optimization:
1. **Minimality** - processing each target only once 

2. **Parallelism** - executing independent build steps in parallel to reduce overall build times

3. **Incrementality** - tracking file changes and dependecy relationships to avoid unnecessary rebuilds on unchanged components

Hermeticity is also an important dimension, but we backlog this in favor for key functionality concerning the other three dimensions. Ideally, we would like to have a container image per language toolchain. 

## Project Structure

The project is organized as a Rust workspace with the following crates:
- **`bagel_cli`** - Command-line interface and entry point
- **`bagel_core`** - Core build system logic, dependency graph management, and scheduling algorithms  
- **`bagel_exec`** - Execution engine for running build commands and managing process lifecycle
- **`bagel_utils`** - Shared utilities and helper functions

## Getting Started

```bash
# Build the project
cargo build

# Run the CLI
cargo run --bin bagel_cli

# Run tests
cargo test
```

### References
A lot of the design decisions made in this project are based on the following work:
- [Build Systems A La Carte](https://simon.peytonjones.org/assets/pdfs/build-systems-jfp.pdf)
- [Bazel](https://bazel.build/)
- [Buck](https://buck2.build/)