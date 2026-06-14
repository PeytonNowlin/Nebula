use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Mutex;

use jsonschema::{Resource, Retrieve, Uri, Validator};
use nebula_host::Host;
use nebula_load::load_workspace;
use nebula_runtime::Runtime;
use nebula_syntax::parse;
use nebula_test_support::workspace_root;
use nebula_types::{report_with_source, typecheck};
use serde_json::Value as JsonValue;

static RUNBOOK_TEST_LOCK: Mutex<()> = Mutex::new(());

struct SchemasDirRetriever {
    by_name: HashMap<String, JsonValue>,
}

impl SchemasDirRetriever {
    fn new() -> Self {
        let dir = workspace_root().join("schemas");
        let by_name = fs::read_dir(&dir)
            .expect("read schemas dir")
            .filter_map(|entry| entry.ok())
            .filter_map(|entry| {
                let path = entry.path();
                if path.extension().and_then(|ext| ext.to_str()) != Some("json") {
                    return None;
                }
                let name = path.file_name()?.to_str()?.to_string();
                let schema: JsonValue =
                    serde_json::from_str(&fs::read_to_string(&path).expect("read schema"))
                        .expect("parse schema");
                Some((name, schema))
            })
            .collect();
        Self { by_name }
    }
}

impl Retrieve for SchemasDirRetriever {
    fn retrieve(
        &self,
        uri: &Uri<String>,
    ) -> Result<JsonValue, Box<dyn std::error::Error + Send + Sync>> {
        let path = uri.path().as_str().trim_start_matches('/');
        let name = path.rsplit('/').next().unwrap_or(path);
        self.by_name
            .get(name)
            .cloned()
            .ok_or_else(|| format!("schema not found: {name}").into())
    }
}

fn load_schema(name: &str) -> Validator {
    let path = workspace_root().join("schemas").join(name);
    let schema: JsonValue = serde_json::from_str(&fs::read_to_string(&path).expect("read schema"))
        .expect("parse schema");
    let retriever = SchemasDirRetriever::new();
    let resources: Vec<_> = retriever
        .by_name
        .iter()
        .map(|(id, contents)| {
            (
                id.clone(),
                Resource::from_contents(contents.clone()).expect("schema resource"),
            )
        })
        .collect();
    jsonschema::options()
        .with_resources(resources.into_iter())
        .with_retriever(retriever)
        .build(&schema)
        .expect("compile schema")
}

fn test_temp(stem: &str, suffix: &str) -> PathBuf {
    std::env::temp_dir().join(format!("nebula-runbook-{stem}-{suffix}.jsonl"))
}

fn write_runbook_manifest(stem: &str, probe_log: &Path, mock_mcp: &Path) -> PathBuf {
    let health_script = workspace_root().join("scripts/runbook_fetch_status.py");
    let manifest_path = std::env::temp_dir().join(format!("nebula-runbook-{stem}-manifest.json"));
    fs::write(
        &manifest_path,
        format!(
            r#"{{
  "mcp_servers": {{
    "local": {{
      "transport": "stdio",
      "command": ["python3", "{}"]
    }}
  }},
  "probes": {{
    "fetch_status": {{
      "kind": "command",
      "command": ["python3", "{}"]
    }},
    "log": {{
      "kind": "jsonl",
      "path": "{}"
    }},
    "notify": {{
      "kind": "mcp",
      "server": "local",
      "tool": "notify"
    }}
  }}
}}"#,
            mock_mcp.display(),
            health_script.display(),
            probe_log.display()
        ),
    )
    .expect("write manifest");
    manifest_path
}

fn validate_jsonl(path: &PathBuf, schema_name: &str) {
    let validator = load_schema(schema_name);
    let source = fs::read_to_string(path).expect("read jsonl file");
    for line in source.lines().filter(|line| !line.trim().is_empty()) {
        let event: JsonValue = serde_json::from_str(line).expect("parse jsonl line");
        validator
            .validate(&event)
            .unwrap_or_else(|err| panic!("schema validation failed: {err}\nline: {line}"));
    }
}

#[test]
fn runbook_example_typechecks() {
    let path = workspace_root().join("examples/runbook.neb");
    let source = fs::read_to_string(&path).expect("read runbook");
    let program = parse(&source).map_err(|err| report_with_source(&path, &source, err));
    let program = program.expect("parse runbook");
    let loaded =
        load_workspace(&path, program).map_err(|err| report_with_source(&path, &source, err));
    let loaded = loaded.expect("load runbook workspace");
    typecheck(&loaded.merged).expect("typecheck runbook");
}

#[test]
fn runbook_happy_path_records_telemetry_and_probe_events() {
    let _guard = RUNBOOK_TEST_LOCK.lock().unwrap();
    let health_log = test_temp("happy", "health");
    let probe_log = test_temp("happy", "probes");
    let telemetry_log = test_temp("happy", "telemetry");
    let _ = fs::remove_file(&health_log);
    let _ = fs::remove_file(&probe_log);
    let _ = fs::remove_file(&telemetry_log);

    std::env::set_var(
        "NEBULA_RUNBOOK_HEALTH_LOG",
        health_log.to_string_lossy().as_ref(),
    );

    let mock_mcp = workspace_root().join("scripts/mcp_mock_stdio.py");
    let manifest_path = write_runbook_manifest("happy", &probe_log, &mock_mcp);
    let runbook = workspace_root().join("examples/runbook.neb");

    let host = Host::with_config(nebula_host::HostConfig {
        probe_manifest: Some(manifest_path),
        telemetry_path: Some(telemetry_log.clone()),
        ..Default::default()
    });
    let result = host.run_file(&runbook);
    assert!(result.ok, "runbook failed: {:?}", result.diagnostics);
    assert_eq!(result.printed, vec!["ready"]);

    validate_jsonl(&telemetry_log, "telemetry-event.schema.json");
    validate_jsonl(&probe_log, "probe-jsonl-event.schema.json");

    let telemetry_source = fs::read_to_string(&telemetry_log).expect("read telemetry");
    let telemetry_lines: Vec<_> = telemetry_source
        .lines()
        .filter(|line| !line.trim().is_empty())
        .collect();
    assert!(
        telemetry_lines.len() >= 6,
        "expected telemetry for let/set/probe steps, got {}",
        telemetry_lines.len()
    );
    let first_set: JsonValue = serde_json::from_str(
        telemetry_lines
            .iter()
            .find(|line| line.contains("\"set\""))
            .expect("set line"),
    )
    .expect("parse set telemetry");
    assert_eq!(first_set["detail"], "attempts");
    assert!(
        first_set.get("value").is_some(),
        "set telemetry should include value"
    );
    let first_probe: JsonValue = serde_json::from_str(
        telemetry_lines
            .iter()
            .find(|line| line.contains("\"probe\""))
            .expect("probe line"),
    )
    .expect("parse probe telemetry");
    assert!(
        first_probe.get("args").is_some(),
        "probe telemetry should include args"
    );
    assert!(
        first_probe.get("result").is_some(),
        "probe telemetry should include result"
    );

    let probe_source = fs::read_to_string(&probe_log).expect("read probe log");
    let probe_lines: Vec<_> = probe_source
        .lines()
        .filter(|line| !line.trim().is_empty())
        .collect();
    assert_eq!(probe_lines.len(), 3, "expected 3 jsonl log calls");

    let health_source = fs::read_to_string(&health_log).expect("read health log");
    let health_lines: Vec<_> = health_source
        .lines()
        .filter(|line| !line.trim().is_empty())
        .collect();
    assert_eq!(health_lines.len(), 3);
    let last: JsonValue = serde_json::from_str(health_lines.last().expect("last health line"))
        .expect("parse health line");
    assert_eq!(last["attempt"], 3);
    assert_eq!(last["status"], 200, "ready attempt should return HTTP 200");

    std::env::remove_var("NEBULA_RUNBOOK_HEALTH_LOG");
}

#[test]
fn runbook_failure_path_notifies_after_retries() {
    let _guard = RUNBOOK_TEST_LOCK.lock().unwrap();
    let probe_log = test_temp("failure", "probes");
    let _ = fs::remove_file(&probe_log);

    let mock_mcp = workspace_root().join("scripts/mcp_mock_stdio.py");
    let manifest_path = write_runbook_manifest("failure", &probe_log, &mock_mcp);

    // Force the service to never become ready within max_attempts so the
    // retries-exhausted path runs (fetch_status returns 503 every attempt).
    std::env::set_var("NEBULA_RUNBOOK_READY_AT", "99");

    // The shipped example, run as-is: readiness is driven by the captured
    // fetch_status return value, not simulated state.
    let src = fs::read_to_string(workspace_root().join("examples/runbook.neb"))
        .expect("read runbook example");

    let ir = Host::new()
        .try_compile_source(&src, None)
        .expect("compile failure runbook")
        .ir;
    let mut runtime = Runtime::new(&ir)
        .with_capture_print(true)
        .with_probe_manifest(&manifest_path, None)
        .expect("load manifest");
    runtime.run(&ir).expect("run failure runbook");

    std::env::remove_var("NEBULA_RUNBOOK_READY_AT");

    assert_eq!(runtime.take_printed(), vec!["failed"]);
    let probe_source = fs::read_to_string(&probe_log).expect("read probe log");
    let probe_lines: Vec<_> = probe_source
        .lines()
        .filter(|line| !line.trim().is_empty())
        .collect();
    assert_eq!(
        probe_lines.len(),
        3,
        "expected 3 jsonl log calls before failure notify"
    );
}
