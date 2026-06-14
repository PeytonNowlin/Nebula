use std::collections::HashMap;
use std::fs;
use std::io::{Read, Write};
use std::net::TcpListener;
use std::path::PathBuf;
use std::thread;

use nebula_ir::lower;
use nebula_runtime::{ProbeHost, ProbeInvocation, RegistryProbeHost, Runtime, Value};
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
        .with_probe_manifest(&manifest_path, None)
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
fn probe_call_expression_returns_value() {
    let script = workspace_root().join("scripts/probe_status.py");
    let manifest_path = std::env::temp_dir().join("nebula-probe-return.json");
    fs::write(
        &manifest_path,
        format!(
            r#"{{
  "probes": {{
    "fetch_status": {{
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
  probe fetch_status(url: Str) -> Int;
  let status: Int = call fetch_status(url: "https://example.com");
  print(int_to_str(status));
}
"#;
    let program = parse(src).expect("parse");
    let typed = typecheck(&program).expect("typecheck");
    let ir = lower(&typed).expect("lower");

    let mut runtime = Runtime::new(&ir)
        .with_probe_manifest(&manifest_path, None)
        .expect("load probe manifest")
        .with_capture_print(true);
    runtime.run(&ir).expect("probe call expression should run");
    assert_eq!(runtime.take_printed(), vec!["200"]);
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
        .with_probe_manifest(&manifest_path, None)
        .expect("load probe manifest");
    runtime.run(&ir).expect("command probe should succeed");
}

fn load_bundle_host() -> RegistryProbeHost {
    let manifest = workspace_root().join("probes/bundle.json");
    let mut host = RegistryProbeHost::with_defaults();
    host.load_manifest(&manifest, None)
        .expect("load bundle manifest");
    host
}

#[test]
fn bundle_probes_require_manifest() {
    let mut host = RegistryProbeHost::with_defaults();
    let err = host
        .invoke(&ProbeInvocation {
            name: "read_file",
            args: HashMap::from([("path".into(), Value::Str("x".into()))]),
        })
        .expect_err("bundle probes should not be registered by default");
    assert!(err.to_string().contains("NEB-P002"));
}

#[test]
fn bundle_read_file_returns_content() {
    let file = std::env::temp_dir().join("nebula-bundle-read.txt");
    fs::write(&file, "hello bundle").expect("write temp file");

    let mut host = load_bundle_host();
    let value = host
        .invoke(&ProbeInvocation {
            name: "read_file",
            args: HashMap::from([("path".into(), Value::Str(file.display().to_string()))]),
        })
        .expect("read_file");
    assert!(matches!(value, Value::Str(content) if content == "hello bundle"));
}

#[test]
fn bundle_write_file_persists_content() {
    let file = std::env::temp_dir().join("nebula-bundle-write.txt");
    let _ = fs::remove_file(&file);

    let mut host = load_bundle_host();
    host.invoke(&ProbeInvocation {
        name: "write_file",
        args: HashMap::from([
            ("path".into(), Value::Str(file.display().to_string())),
            ("content".into(), Value::Str("written".into())),
        ]),
    })
    .expect("write_file");

    let content = fs::read_to_string(&file).expect("read written file");
    assert_eq!(content, "written");
}

#[test]
fn bundle_json_parse_returns_map() {
    let mut host = load_bundle_host();
    let value = host
        .invoke(&ProbeInvocation {
            name: "json_parse",
            args: HashMap::from([(
                "text".into(),
                Value::Str(r#"{"mode":"dry-run","count":2}"#.into()),
            )]),
        })
        .expect("json_parse");
    let Value::Map(map) = value else {
        panic!("expected map, got {value:?}");
    };
    assert!(matches!(map.get("mode"), Some(Value::Str(mode)) if mode == "dry-run"));
    assert!(matches!(map.get("count"), Some(Value::Int(2))));
}

#[test]
fn bundle_env_get_returns_option_str() {
    let key = "NEBULA_BUNDLE_PROBE_TEST";
    std::env::set_var(key, "ready");

    let mut host = load_bundle_host();
    let present = host
        .invoke(&ProbeInvocation {
            name: "env_get",
            args: HashMap::from([("key".into(), Value::Str(key.into()))]),
        })
        .expect("env_get present");
    assert!(matches!(
        present,
        Value::Some(inner) if matches!(*inner, Value::Str(ref s) if s == "ready")
    ));

    let missing = host
        .invoke(&ProbeInvocation {
            name: "env_get",
            args: HashMap::from([(
                "key".into(),
                Value::Str("NEBULA_BUNDLE_PROBE_MISSING".into()),
            )]),
        })
        .expect("env_get missing");
    assert!(matches!(missing, Value::None));
}

#[test]
fn bundle_http_get_fetches_response_body() {
    let listener = TcpListener::bind("127.0.0.1:0").expect("bind test server");
    let port = listener.local_addr().expect("local addr").port();
    let server = thread::spawn(move || {
        if let Ok((mut stream, _)) = listener.accept() {
            let mut buf = [0u8; 1024];
            let _ = stream.read(&mut buf);
            let body = "hello-http";
            let response = format!(
                "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
                body.len()
            );
            let _ = stream.write_all(response.as_bytes());
        }
    });

    let mut host = load_bundle_host();
    let value = host
        .invoke(&ProbeInvocation {
            name: "http_get",
            args: HashMap::from([(
                "url".into(),
                Value::Str(format!("http://127.0.0.1:{port}/")),
            )]),
        })
        .expect("http_get");
    server.join().expect("join server");
    assert!(matches!(value, Value::Str(body) if body == "hello-http"));
}

#[test]
fn bundle_probe_expression_round_trip() {
    let config = workspace_root().join("examples/io_config.json");
    let manifest = workspace_root().join("probes/bundle.json");
    let src = format!(
        r#"
mission main {{
  probe read_file(path: Str) -> Str;
  probe json_parse(text: Str) -> Map<Str, Str>;
  let raw: Str = call read_file(path: "{}");
  let cfg: Map<Str, Str> = call json_parse(text: raw);
  if has(cfg, "mode") and get(cfg, "mode") eq "dry-run" then
    print("dry-run");
  else
    print("live");
  end
}}
"#,
        config.display()
    );

    let program = parse(&src).expect("parse");
    let typed = typecheck(&program).expect("typecheck");
    let ir = lower(&typed).expect("lower");
    let mut runtime = Runtime::new(&ir)
        .with_probe_manifest(&manifest, None)
        .expect("load manifest")
        .with_capture_print(true);
    runtime.run(&ir).expect("run io bundle example");
    assert_eq!(runtime.take_printed(), vec!["dry-run"]);
}
