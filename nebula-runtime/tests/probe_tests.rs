use std::fs;
use std::path::PathBuf;

use nebula_ir::lower;
use nebula_runtime::{ProbeInvocation, ProbeHost, RegistryProbeHost, Runtime};
use nebula_syntax::parse;
use nebula_types::typecheck;

fn workspace_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("workspace root")
        .to_path_buf()
}

#[test]
fn log_probe_emits_structured_jsonl() {
    let log_path = std::env::temp_dir().join("nebula-probe-log.jsonl");
    let _ = fs::remove_file(&log_path);

    let manifest_path = std::env::temp_dir().join("nebula-probe-manifest.json");
    fs::write(
        &manifest_path,
        format!(
            r#"{{
  "probes": {{
    "log": {{
      "kind": "jsonl",
      "path": "{}"
    }}
  }}
}}"#,
            log_path.display()
        ),
    )
    .expect("write manifest");

    let src = r#"
mission main {
  probe log(level: Str, message: Str) -> Void;
  call log(level: "info", message: "hello");
}
"#;
    let program = parse(src).expect("parse");
    let typed = typecheck(&program).expect("typecheck");
    let ir = lower(&typed).expect("lower");

    let mut runtime = Runtime::new(&ir)
        .with_probe_manifest(&manifest_path)
        .expect("load probe manifest");
    runtime.run(&ir).expect("run");

    let lines = fs::read_to_string(&log_path).expect("read probe log");
    let line = lines.lines().next().expect("probe log line");
    let event: serde_json::Value = serde_json::from_str(line).expect("json line");
    assert_eq!(event["probe"], "log");
    assert_eq!(event["args"]["level"], "info");
    assert_eq!(event["args"]["message"], "hello");
}

#[test]
fn undeclared_host_handler_reports_not_implemented() {
    let mut host = RegistryProbeHost::with_defaults();
    let err = host
        .invoke(&ProbeInvocation {
            name: "search",
            args: std::collections::HashMap::new(),
        })
        .expect_err("missing handler should fail");
    assert!(err.to_string().contains("NEB-P002"));
}

#[test]
fn command_probe_invokes_external_handler() {
    let script = workspace_root().join("scripts/probe_ok.py");
    let manifest_path = std::env::temp_dir().join("nebula-command-probe.json");
    fs::write(
        &manifest_path,
        format!(
            r#"{{
  "probes": {{
    "notify": {{
      "kind": "command",
      "command": ["python3", "{}"]
    }}
  }}
}}"#,
            script.display()
        ),
    )
    .expect("write manifest");

    let src = r#"
mission main {
  probe notify(channel: Str, message: Str) -> Void;
  call notify(channel: "ops", message: "ready");
}
"#;
    let program = parse(src).expect("parse");
    let typed = typecheck(&program).expect("typecheck");
    let ir = lower(&typed).expect("lower");

    let mut runtime = Runtime::new(&ir)
        .with_probe_manifest(&manifest_path)
        .expect("load probe manifest");
    runtime.run(&ir).expect("command probe should succeed");
}