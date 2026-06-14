use std::fs;

use nebula_ir::lower;
use nebula_runtime::Runtime;
use nebula_syntax::parse;
use nebula_types::typecheck;
use serde_json::Value as JsonValue;

fn run_with_telemetry(src: &str) -> (Vec<JsonValue>, Vec<String>) {
    let program = parse(src).expect("parse");
    let typed = typecheck(&program).expect("typecheck");
    let ir = lower(&typed).expect("lower");

    let telemetry_path = std::env::temp_dir().join(format!(
        "nebula-telemetry-{}",
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .expect("clock")
            .as_nanos()
    ));
    let _ = fs::remove_file(&telemetry_path);

    let mut runtime =
        Runtime::new(&ir).with_telemetry(telemetry_path.to_string_lossy().into_owned());
    runtime.run(&ir).expect("run");

    let source = fs::read_to_string(&telemetry_path).expect("read telemetry");
    let events: Vec<JsonValue> = source
        .lines()
        .filter(|line| !line.trim().is_empty())
        .map(|line| serde_json::from_str(line).expect("parse telemetry line"))
        .collect();
    let printed = runtime.take_printed();
    (events, printed)
}

#[test]
fn telemetry_set_includes_binding_value() {
    let src = r#"
mission main {
  let mut count: Int = 0;
  telemetry
    set count = count plus 1;
  end
}
"#;
    let (events, _) = run_with_telemetry(src);
    assert_eq!(events.len(), 1);
    assert_eq!(events[0]["step"], "set");
    assert_eq!(events[0]["detail"], "count");
    assert_eq!(events[0]["value"], 1);
}

#[test]
fn telemetry_probe_includes_args_and_result() {
    let src = r#"
mission main {
  probe log(level: Str, message: Str) -> Void;
  telemetry
    call log(level: "info", message: "ready");
  end
}
"#;
    let (events, _) = run_with_telemetry(src);
    assert_eq!(events.len(), 1);
    assert_eq!(events[0]["step"], "probe");
    assert_eq!(events[0]["detail"], "log");
    assert_eq!(events[0]["args"]["level"], "info");
    assert_eq!(events[0]["args"]["message"], "ready");
    assert!(events[0]["result"].is_null());
}

#[test]
fn telemetry_probe_summarizes_string_result() {
    let src = r#"
mission main {
  probe echo(text: Str) -> Str;
  telemetry
    let reply: Str = call echo(text: "hi");
  end
}
"#;
    let manifest_path = std::env::temp_dir().join("nebula-telemetry-echo-manifest.json");
    fs::write(
        &manifest_path,
        r#"{
  "probes": {
    "echo": {
      "kind": "command",
      "command": ["python3", "-c", "import json,sys; req=json.load(sys.stdin); json.dump({'status':'ok','value':req['args']['text']}, sys.stdout)"]
    }
  }
}"#,
    )
    .expect("write manifest");

    let program = parse(src).expect("parse");
    let typed = typecheck(&program).expect("typecheck");
    let ir = lower(&typed).expect("lower");
    let telemetry_path = std::env::temp_dir().join("nebula-telemetry-echo.jsonl");
    let _ = fs::remove_file(&telemetry_path);

    let mut runtime = Runtime::new(&ir)
        .with_probe_manifest(&manifest_path, None)
        .expect("load manifest")
        .with_telemetry(telemetry_path.to_string_lossy().into_owned());
    runtime.run(&ir).expect("run");

    let source = fs::read_to_string(&telemetry_path).expect("read telemetry");
    let probe_event: JsonValue = source
        .lines()
        .filter(|line| !line.trim().is_empty())
        .map(|line| serde_json::from_str(line).expect("parse line"))
        .find(|event: &JsonValue| event["step"] == "probe")
        .expect("probe telemetry event");
    assert_eq!(probe_event["args"]["text"], "hi");
    assert_eq!(probe_event["result"], "hi");
}

#[test]
fn telemetry_let_includes_initial_value() {
    let src = r#"
mission main {
  telemetry
    let count: Int = 2;
  end
}
"#;
    let (events, _) = run_with_telemetry(src);
    assert_eq!(events[0]["step"], "let");
    assert_eq!(events[0]["value"], 2);
}
