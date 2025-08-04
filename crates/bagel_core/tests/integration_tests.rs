use bagel_core::{BuildSpec, BuildSpecError, TargetKind};

#[test]
fn test_empty_toml_succeeds_with_no_targets() {
    let result = BuildSpec::from_toml("");
    assert!(result.is_ok());
    let spec = result.unwrap();
    assert_eq!(spec.targets.len(), 0);
}

#[test]
fn test_minimal_valid_target() {
    let toml = r#"
[hello]
cmd = "echo hello"
inputs = ["input.txt"]
outputs = ["output.txt"]
"#;

    let spec = BuildSpec::from_toml(toml).unwrap();
    assert_eq!(spec.targets.len(), 1);

    let target = spec.get_target("hello").unwrap();
    assert_eq!(target.cmd, "echo hello");
    assert_eq!(target.inputs, vec!["input.txt"]);
    assert_eq!(target.outputs, vec!["output.txt"]);
    assert_eq!(target.kind, TargetKind::Binary);
    assert!(target.deps.is_empty());
    assert!(target.env.is_empty());
}

#[test]
fn test_realistic_c_build() {
    let toml = r#"
[hello_world]
cmd = "gcc -o hello hello.c"
inputs = ["hello.c"]
outputs = ["hello"]
kind = "binary"

[hello_world.env]
CFLAGS = "-Wall -O2"
"#;

    let spec = BuildSpec::from_toml(toml).unwrap();
    let target = spec.get_target("hello_world").unwrap();

    assert_eq!(target.kind, TargetKind::Binary);
    assert_eq!(target.env.get("CFLAGS"), Some(&"-Wall -O2".to_string()));
}

#[test]
fn test_dependency_validation() {
    let toml = r#"
[app]
cmd = "gcc -o app main.c -lmath"
inputs = ["main.c"]
outputs = ["app"]
deps = ["libmath"]

[libmath]
cmd = "gcc -c math.c && ar rcs libmath.a math.o"
inputs = ["math.c"]
outputs = ["libmath.a"]
kind = "lib"
"#;

    let spec = BuildSpec::from_toml(toml).unwrap();
    assert_eq!(spec.targets.len(), 2);

    let app = spec.get_target("app").unwrap();
    assert_eq!(app.deps, vec!["libmath"]);

    let lib = spec.get_target("libmath").unwrap();
    assert_eq!(lib.kind, TargetKind::Lib);
}

#[test]
fn test_missing_dependency_fails() {
    let toml = r#"
[app]
cmd = "build app"
inputs = ["app.c"]
outputs = ["app"]
deps = ["nonexistent"]
"#;

    let result = BuildSpec::from_toml(toml);
    assert!(result.is_err());

    if let Err(BuildSpecError::InvalidTarget(msg)) = result {
        assert!(msg.contains("depends on non-existent target"));
    } else {
        panic!("Expected InvalidTarget error");
    }
}

#[test]
fn test_missing_required_fields() {
    let toml = r#"
[target]
inputs = ["file.c"]
outputs = ["file.o"]
"#;
    assert!(BuildSpec::from_toml(toml).is_err());

    let toml = r#"
[target]
cmd = "gcc file.c"
outputs = ["file.o"]
"#;
    assert!(BuildSpec::from_toml(toml).is_err());

    let toml = r#"
[target]
cmd = "gcc file.c"
inputs = ["file.c"]
"#;
    assert!(BuildSpec::from_toml(toml).is_err());
}
