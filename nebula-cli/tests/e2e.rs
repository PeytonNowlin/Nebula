use std::process::Command;

use nebula_test_support::workspace_root;

fn nebula_bin() -> &'static str {
    env!("CARGO_BIN_EXE_nebula")
}

fn run_example(name: &str) -> std::process::Output {
    Command::new(nebula_bin())
        .arg("run")
        .arg(workspace_root().join("examples").join(name))
        .output()
        .expect("spawn nebula run")
}

#[test]
fn cli_run_hello_prints_greeting() {
    let output = run_example("hello.neb");
    assert!(output.status.success(), "stderr: {}", String::from_utf8_lossy(&output.stderr));
    assert_eq!(
        String::from_utf8_lossy(&output.stdout).trim(),
        "Hello from Nebula"
    );
}

#[test]
fn cli_run_push_demo_prints_lengths() {
    let output = run_example("push_demo.neb");
    assert!(output.status.success(), "stderr: {}", String::from_utf8_lossy(&output.stderr));
    assert_eq!(
        String::from_utf8_lossy(&output.stdout).trim(),
        "5\n3"
    );
}

#[test]
fn cli_run_import_demo_prints_math_results() {
    let output = run_example("import_demo.neb");
    assert!(output.status.success(), "stderr: {}", String::from_utf8_lossy(&output.stderr));
    assert_eq!(
        String::from_utf8_lossy(&output.stdout).trim(),
        "42\n21"
    );
}

#[test]
fn cli_check_passes_on_valid_example() {
    let output = Command::new(nebula_bin())
        .arg("check")
        .arg(workspace_root().join("examples/fizzbuzz.neb"))
        .output()
        .expect("spawn nebula check");
    assert!(output.status.success(), "stderr: {}", String::from_utf8_lossy(&output.stderr));
    assert!(String::from_utf8_lossy(&output.stdout).contains("ok:"));
}

#[test]
fn cli_check_fails_on_type_error() {
    let path = workspace_root().join("examples/hello.neb");
    let mut bad = std::fs::read_to_string(&path).expect("read hello");
    bad.push_str("\nmission broken { let x: Int = \"nope\"; }\n");
    let bad_path = std::env::temp_dir().join("nebula-bad-check.neb");
    std::fs::write(&bad_path, bad).expect("write temp file");

    let output = Command::new(nebula_bin())
        .arg("check")
        .arg(&bad_path)
        .output()
        .expect("spawn nebula check");
    assert!(!output.status.success());
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("NEB-T002"), "expected type mismatch in stderr: {stderr}");
}