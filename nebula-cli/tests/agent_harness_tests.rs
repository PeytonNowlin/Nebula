//! Guards the agent authoring-loop harness (scripts/nebula_agent.py): the single
//! Python entry point an agent drives for generate -> check -> fix -> run.

use std::path::PathBuf;
use std::process::Command;

use serde_json::Value;

fn workspace_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("workspace root")
        .to_path_buf()
}

/// Run `scripts/nebula_agent.py` with the freshly built CLI wired in via
/// $NEBULA_BIN, from the workspace root. Returns (parsed stdout JSON, exit code).
fn harness(args: &[&str]) -> (Value, i32) {
    let root = workspace_root();
    let script = root.join("scripts/nebula_agent.py");
    let output = Command::new("python3")
        .arg(&script)
        .args(args)
        .env("NEBULA_BIN", env!("CARGO_BIN_EXE_nebula"))
        .current_dir(&root)
        .output()
        .expect("run nebula_agent.py");
    let stdout = String::from_utf8_lossy(&output.stdout);
    let value: Value = serde_json::from_str(stdout.trim())
        .unwrap_or_else(|_| panic!("harness did not emit JSON: {stdout}"));
    (value, output.status.code().unwrap_or(-1))
}

#[test]
fn harness_loop_runs_a_clean_program() {
    let (v, code) = harness(&["loop", "examples/hello.neb"]);
    assert_eq!(v["stage"], "run");
    assert_eq!(v["ready"], true);
    assert_eq!(v["ok"], true);
    assert_eq!(v["record"]["printed"][0], "Hello from Nebula");
    assert_eq!(code, 0);
}

#[test]
fn harness_loop_surfaces_diagnostics_for_a_broken_program() {
    let dir = std::env::temp_dir().join("nebula-harness-test");
    std::fs::create_dir_all(&dir).expect("create temp dir");
    let bad = dir.join("bad.neb");
    std::fs::write(&bad, "mission main { let x: Int = \"no\"; }").expect("write bad.neb");

    let (v, code) = harness(&["loop", bad.to_str().unwrap()]);
    assert_eq!(v["stage"], "check");
    assert_eq!(v["ready"], false);
    assert_eq!(v["diagnostics"][0]["code"], "NEB-T002");
    assert_eq!(code, 1);
}
