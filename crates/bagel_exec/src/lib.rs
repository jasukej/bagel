//! Build execution for bagel
//!
//! Provides serial and parallel executors for building targets.

mod parallel;
mod serial;
mod types;

pub use parallel::ParallelExecutor;
pub use serial::SerialExecutor;
pub use types::{BuildReport, ExecConfig, ExecError, TargetResult, TargetStatus};

#[cfg(test)]
mod tests {
    use super::*;
    use bagel_core::BuildSpec;
    use std::path::PathBuf;
    use std::time::Duration;

    fn temp_dir(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("bagel_exec_test_{}", name));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn test_build_report_counts() {
        let report = BuildReport {
            results: vec![
                TargetResult {
                    target_name: "a".to_string(),
                    status: TargetStatus::Built,
                    duration: Duration::from_secs(1),
                    output: None,
                },
                TargetResult {
                    target_name: "b".to_string(),
                    status: TargetStatus::Skipped,
                    duration: Duration::from_millis(10),
                    output: None,
                },
                TargetResult {
                    target_name: "c".to_string(),
                    status: TargetStatus::Failed(1),
                    duration: Duration::from_secs(2),
                    output: None,
                },
            ],
            total_duration: Duration::from_secs(3),
        };

        assert_eq!(report.built_count(), 1);
        assert_eq!(report.skipped_count(), 1);
        assert_eq!(report.failed_count(), 1);
        assert!(!report.success());
    }

    #[test]
    fn test_serial_simple_command() {
        let dir = temp_dir("serial_simple");

        let toml = r#"
            [hello]
            cmd = "echo 'Hello, World!'"
            inputs = ["input.txt"]
            outputs = ["output.txt"]
        "#;

        std::fs::write(dir.join("input.txt"), "test").unwrap();

        let spec = BuildSpec::from_toml(toml).unwrap();
        let config = ExecConfig::new(&dir);
        let mut executor = SerialExecutor::new(config).unwrap();

        let report = executor.execute_all(&spec).unwrap();

        assert_eq!(report.built_count(), 1);
        assert!(report.success());

        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn test_serial_skips_cached() {
        let dir = temp_dir("serial_cached");

        let toml = r#"
            [hello]
            cmd = "echo 'Hello'"
            inputs = ["input.txt"]
            outputs = ["output.txt"]
        "#;

        std::fs::write(dir.join("input.txt"), "test").unwrap();

        let spec = BuildSpec::from_toml(toml).unwrap();
        let config = ExecConfig::new(&dir);

        // First run: should build
        {
            let mut executor = SerialExecutor::new(config.clone()).unwrap();
            let report = executor.execute_all(&spec).unwrap();
            assert_eq!(report.built_count(), 1);
            assert_eq!(report.skipped_count(), 0);
        }

        // Second run: should skip (cached)
        {
            let mut executor = SerialExecutor::new(config.clone()).unwrap();
            let report = executor.execute_all(&spec).unwrap();
            assert_eq!(report.built_count(), 0);
            assert_eq!(report.skipped_count(), 1);
        }

        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn test_parallel_simple_command() {
        let dir = temp_dir("parallel_simple");

        let toml = r#"
            [hello]
            cmd = "echo 'Hello'"
            inputs = ["input.txt"]
            outputs = ["output.txt"]
        "#;

        std::fs::write(dir.join("input.txt"), "test").unwrap();

        let spec = BuildSpec::from_toml(toml).unwrap();
        let mut config = ExecConfig::new(&dir);
        config.parallel = true;

        let mut executor = ParallelExecutor::new(config).unwrap();
        let report = executor.execute_all(&spec).unwrap();

        assert_eq!(report.built_count(), 1);
        assert!(report.success());

        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn test_parallel_diamond_deps() {
        let dir = temp_dir("parallel_diamond");

        // Diamond: A depends on B and C, both depend on D
        let toml = r#"
            [A]
            cmd = "echo 'A'"
            inputs = ["input.txt"]
            outputs = ["a.out"]
            deps = ["B", "C"]

            [B]
            cmd = "echo 'B'"
            inputs = ["input.txt"]
            outputs = ["b.out"]
            deps = ["D"]

            [C]
            cmd = "echo 'C'"
            inputs = ["input.txt"]
            outputs = ["c.out"]
            deps = ["D"]

            [D]
            cmd = "echo 'D'"
            inputs = ["input.txt"]
            outputs = ["d.out"]
        "#;

        std::fs::write(dir.join("input.txt"), "test").unwrap();

        let spec = BuildSpec::from_toml(toml).unwrap();
        let mut config = ExecConfig::new(&dir);
        config.parallel = true;

        let mut executor = ParallelExecutor::new(config).unwrap();
        let report = executor.execute_all(&spec).unwrap();

        assert_eq!(report.built_count(), 4);
        assert!(report.success());

        // Verify D was built before B and C, and A was built last
        let names: Vec<&str> = report
            .results
            .iter()
            .map(|r| r.target_name.as_str())
            .collect();

        let pos_d = names.iter().position(|&n| n == "D").unwrap();
        let pos_b = names.iter().position(|&n| n == "B").unwrap();
        let pos_c = names.iter().position(|&n| n == "C").unwrap();
        let pos_a = names.iter().position(|&n| n == "A").unwrap();

        assert!(pos_d < pos_b, "D should be built before B");
        assert!(pos_d < pos_c, "D should be built before C");
        assert!(pos_b < pos_a, "B should be built before A");
        assert!(pos_c < pos_a, "C should be built before A");

        std::fs::remove_dir_all(&dir).ok();
    }
}
