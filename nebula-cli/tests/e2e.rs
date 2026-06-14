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
fn cli_parse_json_exports_ast() {
    let output = Command::new(nebula_bin())
        .arg("parse")
        .arg("--json")
        .arg(workspace_root().join("examples/hello.neb"))
        .output()
        .expect("spawn nebula parse --json");
    assert!(output.status.success(), "stderr: {}", String::from_utf8_lossy(&output.stderr));
    let parsed: serde_json::Value =
        serde_json::from_slice(&output.stdout).expect("stdout should be JSON");
    assert!(parsed["program"]["items"].is_array());
    assert!(parsed["entry"].as_str().unwrap().contains("hello.neb"));
    assert_eq!(parsed["loaded"], false);
}

#[test]
fn cli_parse_json_load_exports_merged_ast() {
    let output = Command::new(nebula_bin())
        .arg("parse")
        .arg("--json")
        .arg("--load")
        .arg(workspace_root().join("examples/import_demo.neb"))
        .output()
        .expect("spawn nebula parse --json --load");
    assert!(output.status.success(), "stderr: {}", String::from_utf8_lossy(&output.stderr));
    let parsed: serde_json::Value =
        serde_json::from_slice(&output.stdout).expect("stdout should be JSON");
    assert_eq!(parsed["loaded"], true);
    let items = parsed["program"]["items"].as_array().expect("items array");
    assert!(items
        .iter()
        .any(|item| item["node"].get("Sector").is_some()));
}

#[test]
fn cli_ir_json_exports_lowered_program() {
    let output = Command::new(nebula_bin())
        .arg("ir")
        .arg("--json")
        .arg(workspace_root().join("examples/hello.neb"))
        .output()
        .expect("spawn nebula ir --json");
    assert!(output.status.success(), "stderr: {}", String::from_utf8_lossy(&output.stderr));
    let parsed: serde_json::Value =
        serde_json::from_slice(&output.stdout).expect("stdout should be JSON");
    assert!(parsed["ir"]["mission"]["name"].as_str().unwrap() == "main");
    assert!(parsed["ir"]["mission"]["stmts"].is_array());
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
fn cli_check_json_emits_structured_diagnostics_on_failure() {
    let path = workspace_root().join("examples/hello.neb");
    let mut bad = std::fs::read_to_string(&path).expect("read hello");
    bad.push_str("\nmission broken { let x: Int = \"nope\"; }\n");
    let bad_path = std::env::temp_dir().join("nebula-bad-check-json.neb");
    std::fs::write(&bad_path, bad).expect("write temp file");

    let output = Command::new(nebula_bin())
        .arg("check")
        .arg("--json")
        .arg(&bad_path)
        .output()
        .expect("spawn nebula check --json");
    assert!(!output.status.success());
    let stderr = String::from_utf8_lossy(&output.stderr);
    let parsed: Vec<serde_json::Value> =
        serde_json::from_str(stderr.trim()).expect("stderr should be JSON array");
    assert_eq!(parsed.len(), 1);
    assert_eq!(parsed[0]["code"], "NEB-T002");
    assert!(parsed[0]["message"].as_str().unwrap().contains("type mismatch"));
    assert!(parsed[0]["span"]["start"].is_number());
}

#[test]
fn cli_check_json_emits_empty_array_on_success() {
    let output = Command::new(nebula_bin())
        .arg("check")
        .arg("--json")
        .arg(workspace_root().join("examples/fizzbuzz.neb"))
        .output()
        .expect("spawn nebula check --json");
    assert!(output.status.success());
    assert_eq!(String::from_utf8_lossy(&output.stdout).trim(), "[]");
}

#[test]
fn cli_run_json_emits_structured_diagnostics_on_type_error() {
    let path = workspace_root().join("examples/hello.neb");
    let mut bad = std::fs::read_to_string(&path).expect("read hello");
    bad.push_str("\nmission broken { let x: Int = \"nope\"; }\n");
    let bad_path = std::env::temp_dir().join("nebula-bad-run-json.neb");
    std::fs::write(&bad_path, bad).expect("write temp file");

    let output = Command::new(nebula_bin())
        .arg("run")
        .arg("--json")
        .arg(&bad_path)
        .output()
        .expect("spawn nebula run --json");
    assert!(!output.status.success());
    let stderr = String::from_utf8_lossy(&output.stderr);
    let parsed: Vec<serde_json::Value> =
        serde_json::from_str(stderr.trim()).expect("stderr should be JSON array");
    assert!(parsed.iter().any(|diag| diag["code"] == "NEB-T002"));
}

#[test]
fn cli_probes_list_json_reports_manifest_bindings() {
    let manifest = workspace_root().join("probes/mcp_stdio.json");
    let output = Command::new(nebula_bin())
        .arg("probes")
        .arg("list")
        .arg("--json")
        .arg("--probes")
        .arg(&manifest)
        .output()
        .expect("spawn nebula probes list --json");
    assert!(output.status.success(), "stderr: {}", String::from_utf8_lossy(&output.stderr));
    let parsed: serde_json::Value =
        serde_json::from_slice(&output.stdout).expect("stdout should be JSON");
    let probes = parsed["probes"].as_array().expect("probes array");
    assert!(probes.iter().any(|probe| probe["name"] == "notify" && probe["kind"] == "mcp"));
    assert!(probes.iter().any(|probe| probe["name"] == "log" && probe["kind"] == "jsonl"));
    assert!(parsed["mcp_servers"].is_null());
}

#[test]
fn cli_probes_list_mcp_discovers_tools() {
    let manifest = workspace_root().join("probes/mcp_stdio.json");
    let output = Command::new(nebula_bin())
        .arg("probes")
        .arg("list")
        .arg("--json")
        .arg("--mcp")
        .arg("--probes")
        .arg(&manifest)
        .output()
        .expect("spawn nebula probes list --json --mcp");
    assert!(output.status.success(), "stderr: {}", String::from_utf8_lossy(&output.stderr));
    let parsed: serde_json::Value =
        serde_json::from_slice(&output.stdout).expect("stdout should be JSON");
    let tools = parsed["mcp_servers"]["local"]["tools"]
        .as_array()
        .expect("local tools array");
    assert!(tools.iter().any(|tool| tool["name"] == "notify"));
}

#[test]
fn cli_run_runbook_deploy_readiness() {
    let output = Command::new(nebula_bin())
        .arg("run")
        .arg(workspace_root().join("examples/runbook.neb"))
        .arg("--probes")
        .arg(workspace_root().join("probes/runbook.json"))
        .output()
        .expect("spawn nebula run runbook");
    assert!(
        output.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert_eq!(
        String::from_utf8_lossy(&output.stdout).trim(),
        "ready"
    );
}

#[test]
fn cli_check_runbook_passes() {
    let output = Command::new(nebula_bin())
        .arg("check")
        .arg("--json")
        .arg(workspace_root().join("examples/runbook.neb"))
        .output()
        .expect("spawn nebula check runbook");
    assert!(output.status.success());
    assert_eq!(String::from_utf8_lossy(&output.stdout).trim(), "[]");
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