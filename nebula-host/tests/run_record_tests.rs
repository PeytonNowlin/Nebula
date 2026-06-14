use nebula_host::{Host, HostConfig, RunRecord};
use nebula_test_support::workspace_root;

#[test]
fn run_record_success_includes_program_and_timing() {
    let host = Host::new();
    let result = host.run_source(r#"mission main { print("hi"); }"#);
    assert!(result.ok);
    assert_eq!(result.record.exit, 0);
    assert_eq!(result.record.program, "<source>");
    assert!(result.record.diagnostics.is_empty());
    assert!(result.record.duration_ms > 0 || result.record.duration_ms == 0);
    assert_eq!(result.record.printed, vec!["hi"]);
    assert_eq!(result.record.return_value, Some(serde_json::Value::Null));
    assert!(result.record.probes_called.is_empty());
}

#[test]
fn run_record_failure_includes_diagnostics() {
    let host = Host::new();
    let result = host.run_source(r#"mission main { let x: Int = 1 div 0; print(int_to_str(x)); }"#);
    assert!(!result.ok);
    assert_eq!(result.record.exit, 1);
    assert_eq!(result.record.diagnostics.len(), 1);
    assert!(result.record.probe_events.is_empty());
}

#[test]
fn run_record_captures_jsonl_probe_events() {
    let telemetry = std::env::temp_dir().join("nebula-host-run-record-telemetry.jsonl");
    let _ = std::fs::remove_file(&telemetry);
    let telemetry_path = telemetry.clone();
    let host = Host::with_config(HostConfig {
        telemetry_path: Some(telemetry_path),
        ..Default::default()
    });
    let result = host.run_source(
        r#"
mission main {
  probe log(level: Str, message: Str) -> Void;
  telemetry
    call log(level: "info", message: "ready");
  end
}
"#,
    );
    assert!(result.ok);
    assert_eq!(result.record.probe_events.len(), 1);
    assert_eq!(result.record.probe_events[0].probe, "log");
    assert_eq!(
        result.record.probe_events[0]
            .args
            .get("message")
            .and_then(|v| v.as_str()),
        Some("ready")
    );
    assert_eq!(
        result.record.telemetry_path.as_deref(),
        Some(telemetry.to_string_lossy().as_ref())
    );
}

#[test]
fn run_file_record_uses_entry_path() {
    let host = Host::new();
    let path = workspace_root().join("examples/hello.neb");
    let result = host.run_file(&path);
    assert!(result.ok);
    assert!(result.record.program.ends_with("examples/hello.neb"));
}

#[test]
fn run_record_schema_shape_matches_expectations() {
    let record = RunRecord::success(
        "examples/hello.neb".into(),
        Some("trace.jsonl".into()),
        Vec::new(),
        12,
        vec!["Hello from Nebula".into()],
        Some(serde_json::Value::Null),
        Vec::new(),
    );
    let json = serde_json::to_value(&record).expect("serialize");
    assert_eq!(json["program"], "examples/hello.neb");
    assert_eq!(json["exit"], 0);
    assert_eq!(json["telemetry_path"], "trace.jsonl");
    assert_eq!(json["duration_ms"], 12);
    assert_eq!(json["printed"], serde_json::json!(["Hello from Nebula"]));
    assert_eq!(json["return_value"], serde_json::Value::Null);
    assert_eq!(json["probes_called"], serde_json::json!([]));
}
