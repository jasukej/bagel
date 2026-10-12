Bagel is a lightweight, parallelizable build system written in Rust that:

- Builds a dependency DAG and schedules commands to run in topological order
- Allows incremental builds by skipping up-to-date targets with hashing
- Supports parallel execution of independent build steps
- Is inspired by Bazel (Google's build system). This is a fun project, and by no means a production-ready system :)

## Introduction

With respect to the design of build systems, Bagel narrows down 3 dimensions of optimization:

1. **Minimality** - processing each target only once

2. **Parallelism** - executing independent build steps in parallel to reduce overall build times

3. **Incrementality** - tracking file changes and dependecy relationships to avoid unnecessary rebuilds on unchanged components

Hermeticity is also an important dimension, but we backlog this in favor for key functionality concerning the other three dimensions. Ideally, we would like to have a container image per language toolchain.

A more detailed discussion can be found in [docs/design.md](docs/design.md).

## Project Structure

The project is a single Rust package (`bagel`) organized into modules:

- **`core`** - build spec parsing, dependency graph management, and topological sort
- **`exec`** - serial and parallel (rayon-backed) executors that actually run commands
- **`utils`** - hashing/caching for incremental builds, and shared helpers

## Getting Started

```bash
# Build the project
cargo build

# Run the CLI without installing anything
cargo run -- build

# Run tests
cargo test
```

### Dependencies
**Required**
- **Rust 1.85+**: We use the 2024 edition.

**Optional**
- **[xxHash](https://github.com/Cyan4973/xxHash) 0.8+** and **pkg-config**: Bagel links against the system xxHash library for fast file hashing. Otherwise, falls back to SHA-256, which is (slightly) slower on large source files.
- **[Graphviz](https://graphviz.org/)**: needed to render the output of `bagel graph` into an image.

```bash
# macOS
brew install xxhash pkg-config graphviz

# Debian/Ubuntu
sudo apt install libxxhash-dev pkg-config graphviz
```

If you install any dependencies after the initial build, re-run with `cargo clean -p bagel`.

**Crates**
- **`rayon`**: thread pool to enable parallel execution
- **`sha2`**, **`hex`**: SHA-256 hashing for cache keys (and the fallback file hash)
- **`glob`**: to expand input patterns in `Bagel.toml`
- **`serde`**, **`toml`**, **`serde_json`**: parse `Bagel.toml` and write out JSON/error reports
- **`thiserror`**: error types
- **`pkg-config`**: find system libraries e.g. xxHash at build time


## Installing
If you're going to run `bagel` more than once or twice, you may want to install it onto your PATH:

```bash
cargo install --path .
bagel build
```

**Note**: The instructions in this README assumes `bagel` is installed. If you'd rather not install it, swap `bagel` for `cargo run --release --` in any command below.

## Command-line flags

### `bagel build`

Builds your project off `Bagel.toml`.

- **`-f`, `--force`** — force rebuild of all targets (no-cache).
- **`-j`, `--parallel`** — run independent targets concurrently. Bagel defaults to sequential execution.
- **`-v`, `--verbose`** — prints metadata (incl. shell command) for each target ran.
- **`--json <path>`** — alongside the normal terminal output, write a full build report to `<path>` as JSON. Provides the build progress, start time relative to the build, duration, and declared dependencies of each target.

### `bagel info`

Parses and prints the contents of a `Bagel.toml`.

### `bagel graph`

Prints the dependency graph as Graphviz DOT to stdout, pass `--out <path>` to write to a file.

```bash
bagel graph --out graph.dot
dot -Tpng graph.dot -o graph.png
```

## Examples

The `examples` directory contains several demo projects that can be built with Bagel. Here is an example workflow to compare the serial and parallel execution of `examples/demo`:

```bash
cd examples/demo
bagel build --json serial.json
bagel build -j --json parallel.json
cat parallel.json
```

### References

A lot of the design decisions made in this project are based on the following work:

- [Build Systems A La Carte](https://simon.peytonjones.org/assets/pdfs/build-systems-jfp.pdf)
- [Bazel](https://bazel.build/)
- [Buck](https://buck2.build/)
