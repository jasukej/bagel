use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::Path;
use thiserror::Error;

/** Errors that can occur during build spec parsing */
#[derive(Error, Debug)]
pub enum BuildSpecError {
    #[error("Failed to read build spec file: {0}")]
    IoError(#[from] std::io::Error),
    #[error("Failed to parse TOML: {0}")]
    TomlError(#[from] toml::de::Error),
    #[error("Invalid target specification: {0}")]
    InvalidTarget(String),
}

/** Kind of build target */
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
#[derive(Default)]
pub enum TargetKind {
    #[default]
    Binary,
    Lib,
}

/**
 * Specification for a single build target
 */
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TargetSpec {
    /** Shell command executed for this target */
    pub cmd: String,

    /** Files (or globs) to hash for change detection */
    pub inputs: Vec<String>,

    /** Files treated as the artifact & cache key */
    pub outputs: Vec<String>,

    /** Other targets that must finish first */
    #[serde(default)]
    pub deps: Vec<String>,

    #[serde(default)]
    pub env: HashMap<String, String>,

    /** Kind of target (binary or lib) */
    #[serde(default)]
    pub kind: TargetKind,
}

impl TargetSpec {
    pub fn validate(&self, target_name: &str) -> Result<(), BuildSpecError> {
        if self.cmd.trim().is_empty() {
            return Err(BuildSpecError::InvalidTarget(format!(
                "Target '{target_name}' has empty command"
            )));
        }

        if self.inputs.iter().any(|s| s.trim().is_empty()) {
            return Err(BuildSpecError::InvalidTarget(format!(
                "Target '{target_name}' has empty input file"
            )));
        }

        if self.outputs.iter().any(|s| s.trim().is_empty()) {
            return Err(BuildSpecError::InvalidTarget(format!(
                "Target '{target_name}' has empty output file"
            )));
        }

        if self.deps.iter().any(|s| s.trim().is_empty()) {
            return Err(BuildSpecError::InvalidTarget(format!(
                "Target '{target_name}' has empty dependency name"
            )));
        }

        if self.inputs.is_empty() {
            return Err(BuildSpecError::InvalidTarget(format!(
                "Target '{target_name}' has no inputs specified"
            )));
        }

        if self.outputs.is_empty() {
            return Err(BuildSpecError::InvalidTarget(format!(
                "Target '{target_name}' has no outputs specified"
            )));
        }

        Ok(())
    }
}

/** Build spec containing all targets */
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BuildSpec {
    #[serde(flatten)]
    pub targets: HashMap<String, TargetSpec>,
}

impl BuildSpec {
    pub fn from_file<P: AsRef<Path>>(path: P) -> Result<Self, BuildSpecError> {
        let content = std::fs::read_to_string(path)?;
        Self::from_toml(&content)
    }

    pub fn from_toml(content: &str) -> Result<Self, BuildSpecError> {
        let spec: BuildSpec = toml::from_str(content)?;
        spec.validate()?;
        Ok(spec)
    }

    pub fn validate(&self) -> Result<(), BuildSpecError> {
        for (name, target) in &self.targets {
            target.validate(name)?;
        }

        self.validate_dependencies()?;

        Ok(())
    }

    /*
     * Validate no circular or non-existent deps.
     */
    fn validate_dependencies(&self) -> Result<(), BuildSpecError> {
        for (target_name, target) in &self.targets {
            for dep in &target.deps {
                if !self.targets.contains_key(dep) {
                    return Err(BuildSpecError::InvalidTarget(format!(
                        "Target '{target_name}' depends on non-existent target '{dep}'"
                    )));
                }

                if dep == target_name {
                    return Err(BuildSpecError::InvalidTarget(format!(
                        "Target '{target_name}' cannot depend on itself"
                    )));
                }
            }
        }

        /** Run topological sort to detect any cycles */
        #[derive(PartialEq)]
        enum State {
            Unvisited,
            Visiting,
            Visited,
        }

        let mut state: HashMap<&str, State> = self
            .targets
            .keys()
            .map(|k| (k.as_str(), State::Unvisited))
            .collect();

        fn dfs<'a>(
            curr: &'a str,
            spec: &'a BuildSpec,
            state: &mut HashMap<&'a str, State>,
        ) -> Result<(), BuildSpecError> {
            match state.get(curr) {
                Some(State::Visiting) => {
                    return Err(BuildSpecError::InvalidTarget(format!(
                        "Circular dependency detected involving target '{curr}'"
                    )));
                }
                Some(State::Visited) => return Ok(()),
                _ => {}
            }

            state.insert(curr, State::Visiting);

            if let Some(target) = spec.targets.get(curr) {
                for dep in &target.deps {
                    dfs(dep, spec, state)?;
                }
            }

            state.insert(curr, State::Visited);
            Ok(())
        }

        for target_name in self.targets.keys() {
            if state.get((target_name).as_str()) == Some(&State::Unvisited) {
                dfs(target_name, self, &mut state)?;
            }
        }

        Ok(())
    }

    pub fn get_target(&self, name: &str) -> Option<&TargetSpec> {
        self.targets.get(name)
    }

    pub fn target_names(&self) -> Vec<&String> {
        self.targets.keys().collect()
    }

    pub fn has_target(&self, name: &str) -> bool {
        self.targets.contains_key(name)
    }

    pub fn topological_sort(&self) -> Result<Vec<String>, BuildSpecError> {
        #[derive(PartialEq, Clone, Copy)]
        enum State {
            Unvisited,
            Visiting,
            Visited,
        }

        let mut state: HashMap<&str, State> = self
            .targets
            .keys()
            .map(|k| (k.as_str(), State::Unvisited))
            .collect();

        let mut result: Vec<String> = Vec::new();

        // dfs; add nodes in post-order (after all dependencies are visited)
        fn dfs<'a>(
            curr: &'a str,
            spec: &'a BuildSpec,
            state: &mut HashMap<&'a str, State>,
            result: &mut Vec<String>,
        ) -> Result<(), BuildSpecError> {
            match state.get(curr) {
                Some(State::Visiting) => {
                    return Err(BuildSpecError::InvalidTarget(format!(
                        "Circular dependency detected involving target '{curr}'"
                    )));
                }
                Some(State::Visited) => return Ok(()),
                _ => {}
            }

            state.insert(curr, State::Visiting);

            // Visit all dependencies first
            if let Some(target) = spec.targets.get(curr) {
                for dep in &target.deps {
                    dfs(dep, spec, state, result)?;
                }
            }

            state.insert(curr, State::Visited);
            result.push(curr.to_string());

            Ok(())
        }

        for target_name in self.targets.keys() {
            if state.get(target_name.as_str()) == Some(&State::Unvisited) {
                dfs(target_name, self, &mut state, &mut result)?;
            }
        }

        Ok(result)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_simple_target() {
        let toml_content = r#"
            [hello_world]
            cmd = "gcc -o hello hello.c"
            inputs = ["hello.c"]
            outputs = ["hello"]
            "#;

        let spec = BuildSpec::from_toml(toml_content).unwrap();
        assert_eq!(spec.targets.len(), 1);

        let target = spec.get_target("hello_world").unwrap();
        assert_eq!(target.cmd, "gcc -o hello hello.c");
        assert_eq!(target.inputs, vec!["hello.c"]);
        assert_eq!(target.outputs, vec!["hello"]);
        assert_eq!(target.kind, TargetKind::Binary);
    }

    #[test]
    fn test_parse_complex_target() {
        let toml_content = r#"
            [my_library]
            cmd = "cargo build --lib"
            inputs = ["src/**/*.rs", "Cargo.toml"]
            outputs = ["target/debug/libmy_library.rlib"]
            deps = ["codegen"]
            kind = "lib"

            [my_library.env]
            RUSTFLAGS = "-C opt-level=2"
            CARGO_TARGET_DIR = "custom_target"

            [codegen]
            cmd = "python generate_code.py"
            inputs = ["templates/*.j2", "schema.yaml"]
            outputs = ["src/generated.rs"]
            "#;

        let spec = BuildSpec::from_toml(toml_content).unwrap();
        assert_eq!(spec.targets.len(), 2);

        let lib_target = spec.get_target("my_library").unwrap();
        assert_eq!(lib_target.kind, TargetKind::Lib);
        assert_eq!(lib_target.deps, vec!["codegen"]);
        assert_eq!(
            lib_target.env.get("RUSTFLAGS"),
            Some(&"-C opt-level=2".to_string())
        );
        assert_eq!(
            lib_target.env.get("CARGO_TARGET_DIR"),
            Some(&"custom_target".to_string())
        );
    }

    #[test]
    fn test_invalid_target_no_required_params() {
        let toml_content = r#"
            [invalid_lib]
            inputs = ["src/**/*.rs", "Cargo.toml"]
            outputs = ["target/debug/libmy_library.rlib"]
            deps = []
            kind = "lib"

            [invalid_lib.env]
            RUSTFLAGS = "-C opt-level=2"
        "#;

        let spec_result = BuildSpec::from_toml(toml_content);
        assert!(spec_result.is_err(), "cmd is required");
    }

    #[test]
    fn test_invalid_target_circular_deps() {
        let toml_content = r#"
            [circular_lib]
            cmd = "cargo build --lib"
            inputs = ["src/**/*.rs", "Cargo.toml"]
            outputs = ["target/debug/libmy_library.rlib"]
            deps = ["a_dep"]

            [a_dep]
            cmd = "python generate_code.py"
            inputs = ["templates/*.j2", "schema.yaml"]
            outputs = ["src/compiled.ts"]
            deps = ["b_dep"]

            [b_dep]
            cmd = "tsc src/*.ts" 
            inputs = ["src/**/*.ts", "Cargo.toml"]
            outputs = ["src/compiled.ts"]
            deps = ["circular_lib"]
        "#;

        let spec_result = BuildSpec::from_toml(toml_content);
        assert!(
            spec_result.is_err(),
            "Circular dependency should be detected"
        )
    }

    #[test]
    fn test_topological_sort_simple() {
        // Linear dependency chain: A -> B -> C
        let toml_content = r#"
            [A]
            cmd = "echo A"
            inputs = ["a.txt"]
            outputs = ["a.out"]
            deps = ["B"]

            [B]
            cmd = "echo B"
            inputs = ["b.txt"]
            outputs = ["b.out"]
            deps = ["C"]

            [C]
            cmd = "echo C"
            inputs = ["c.txt"]
            outputs = ["c.out"]
        "#;

        let spec = BuildSpec::from_toml(toml_content).unwrap();
        let order = spec.topological_sort().unwrap();

        // C must come before B, B must come before A
        let pos_c = order.iter().position(|x| x == "C").unwrap();
        let pos_b = order.iter().position(|x| x == "B").unwrap();
        let pos_a = order.iter().position(|x| x == "A").unwrap();

        assert!(pos_c < pos_b, "C should come before B");
        assert!(pos_b < pos_a, "B should come before A");
    }

    #[test]
    fn test_topological_sort_diamond() {
        // test a damond dependency:
        //     A
        //    / \
        //   B   C
        //    \ /
        //     D
        let toml_content = r#"
            [A]
            cmd = "echo A"
            inputs = ["a.txt"]
            outputs = ["a.out"]
            deps = ["B", "C"]

            [B]
            cmd = "echo B"
            inputs = ["b.txt"]
            outputs = ["b.out"]
            deps = ["D"]

            [C]
            cmd = "echo C"
            inputs = ["c.txt"]
            outputs = ["c.out"]
            deps = ["D"]

            [D]
            cmd = "echo D"
            inputs = ["d.txt"]
            outputs = ["d.out"]
        "#;

        let spec = BuildSpec::from_toml(toml_content).unwrap();
        let order = spec.topological_sort().unwrap();

        // D must come first, B and C can be in any order, A must be last
        let pos_d = order.iter().position(|x| x == "D").unwrap();
        let pos_b = order.iter().position(|x| x == "B").unwrap();
        let pos_c = order.iter().position(|x| x == "C").unwrap();
        let pos_a = order.iter().position(|x| x == "A").unwrap();

        assert_eq!(pos_d, 0, "D should be first (no deps)");
        assert!(pos_b < pos_a, "B should come before A");
        assert!(pos_c < pos_a, "C should come before A");
        assert_eq!(pos_a, 3, "A should be last");
    }

    #[test]
    fn test_topological_sort_independent() {
        // No dependencies between targets
        let toml_content = r#"
            [A]
            cmd = "echo A"
            inputs = ["a.txt"]
            outputs = ["a.out"]

            [B]
            cmd = "echo B"
            inputs = ["b.txt"]
            outputs = ["b.out"]

            [C]
            cmd = "echo C"
            inputs = ["c.txt"]
            outputs = ["c.out"]
        "#;

        let spec = BuildSpec::from_toml(toml_content).unwrap();
        let order = spec.topological_sort().unwrap();

        // All three targets should be in the result
        assert_eq!(order.len(), 3);
        assert!(order.contains(&"A".to_string()));
        assert!(order.contains(&"B".to_string()));
        assert!(order.contains(&"C".to_string()));
    }
}
