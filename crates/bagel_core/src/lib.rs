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

/** Type of build target */
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

    /** Files (or globs) Bagel will hash for change detection */
    pub inputs: Vec<String>,

    /** Files Bagel will treat as the artifact & cache key */
    pub outputs: Vec<String>,

    /** Other Bagel targets that must finish first */
    #[serde(default)]
    pub deps: Vec<String>,

    /** Declared environment variables */
    #[serde(default)]
    pub env: HashMap<String, String>,

    /** Type of target (binary or lib) */
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
     * Validate no circular dependencies. Current version only checks for non-existent dependencies.
     */
    fn validate_dependencies(&self) -> Result<(), BuildSpecError> {
        for (target_name, target) in &self.targets {
            for dep in &target.deps {
                if !self.targets.contains_key(dep) {
                    return Err(BuildSpecError::InvalidTarget(format!(
                        "Target '{target_name}' depends on non-existent target '{dep}'"
                    )));
                }
            }
        }

        // TODO: add cycle detection
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
}
