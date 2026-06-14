use std::collections::HashMap;
use std::fs;
use std::path::PathBuf;

use jsonschema::{Resource, Retrieve, Uri, Validator};
use nebula_ir::lower;
use nebula_runtime::Runtime;
use nebula_syntax::parse;
use nebula_types::typecheck;
use serde_json::Value;

fn workspace_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("workspace root")
        .to_path_buf()
}

fn schema_dir() -> PathBuf {
    workspace_root().join("schemas")
}

struct SchemasDirRetriever {
    by_name: HashMap<String, Value>,
}

impl SchemasDirRetriever {
    fn new() -> Self {
        let dir = schema_dir();
        let by_name = fs::read_dir(&dir)
            .expect("read schemas dir")
            .filter_map(|entry| entry.ok())
            .filter_map(|entry| {
                let path = entry.path();
                if path.extension().and_then(|ext| ext.to_str()) != Some("json") {
                    return None;
                }
                let name = path.file_name()?.to_str()?.to_string();
                let schema: Value =
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
    ) -> Result<Value, Box<dyn std::error::Error + Send + Sync>> {
        let path = uri.path().as_str().trim_start_matches('/');
        let name = path.rsplit('/').next().unwrap_or(path);
        self.by_name
            .get(name)
            .cloned()
            .ok_or_else(|| format!("schema not found: {name}").into())
    }
}

fn load_schema(name: &str) -> Validator {
    let path = schema_dir().join(name);
    let schema: Value = serde_json::from_str(&fs::read_to_string(&path).expect("read schema"))
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

#[test]
fn telemetry_event_schema_matches_runtime_output() {
    let validator = load_schema("telemetry-event.schema.json");
    let src = r#"
mission main {
  probe log(level: Str, message: Str) -> Void;
  let mut count: Int = 0;
  telemetry
    let base: Int = 1;
    set count = count plus 1;
    call log(level: "info", message: "ready");
  end
}
"#;
    let program = parse(src).expect("parse");
    let typed = typecheck(&program).expect("typecheck");
    let ir = lower(&typed).expect("lower");

    let telemetry_path = std::env::temp_dir().join(format!(
        "nebula-schema-telemetry-{}",
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .expect("clock")
            .as_nanos()
    ));
    let _ = fs::remove_file(&telemetry_path);

    let mut runtime =
        Runtime::new(&ir).with_telemetry(telemetry_path.to_string_lossy().into_owned());
    runtime.run(&ir).expect("run");

    let source = fs::read_to_string(&telemetry_path).expect("read telemetry output");
    for line in source.lines().filter(|line| !line.trim().is_empty()) {
        let event: Value = serde_json::from_str(line).expect("parse telemetry line");
        if let Err(err) = validator.validate(&event) {
            panic!("telemetry line failed schema validation: {err}\nline: {line}");
        }
    }
}

#[test]
fn probe_jsonl_event_schema_matches_log_probe_shape() {
    let validator = load_schema("probe-jsonl-event.schema.json");
    let event = serde_json::json!({
        "ts": 1718380800,
        "probe": "log",
        "args": {
            "level": "info",
            "message": "hello"
        }
    });
    validator
        .validate(&event)
        .expect("probe jsonl event should validate");
}

#[test]
fn diagnostic_schema_matches_json_output_shapes() {
    let validator = load_schema("diagnostic.schema.json");

    // Type error with a full span (as emitted by `check --json`).
    let with_span = serde_json::json!({
        "code": "NEB-T002",
        "span": {
            "file": "example.neb",
            "start": 42,
            "end": 58,
            "line": 3,
            "column": 15
        },
        "message": "type mismatch: expected Int, found Str"
    });
    // Runtime error with no source location (`span` omitted entirely).
    let without_span = serde_json::json!({
        "code": "NEB-R004",
        "message": "division by zero"
    });
    let resource_limit = serde_json::json!({
        "code": "NEB-R008",
        "message": "execution exceeded time limit of 30000ms"
    });
    // Span tied to no named file serializes `file` as null.
    let null_file = serde_json::json!({
        "code": "NEB-L002",
        "span": { "file": Value::Null, "start": 0, "end": 0 },
        "message": "circular import"
    });

    for sample in [&with_span, &without_span, &resource_limit, &null_file] {
        validator
            .validate(sample)
            .unwrap_or_else(|_| panic!("diagnostic should validate: {sample}"));
    }
}

#[test]
fn diagnostic_schema_rejects_malformed_records() {
    let validator = load_schema("diagnostic.schema.json");
    let bad = [
        serde_json::json!({ "message": "no code" }),
        serde_json::json!({ "code": "NEB-T002" }),
        serde_json::json!({ "code": "lowercase", "message": "bad code" }),
        serde_json::json!({ "code": "NEB-T002", "message": "x", "extra": 1 }),
        serde_json::json!({ "code": "NEB-T002", "message": "x", "span": { "start": 1, "end": 2 } }),
    ];
    for sample in &bad {
        assert!(
            validator.validate(sample).is_err(),
            "schema should reject malformed diagnostic: {sample}"
        );
    }
}

#[test]
fn probe_manifest_schema_validates_bundle() {
    let validator = load_schema("probe-manifest.schema.json");
    let manifest_path = workspace_root().join("probes/bundle.json");
    let manifest: Value =
        serde_json::from_str(&fs::read_to_string(&manifest_path).expect("read bundle manifest"))
            .expect("parse bundle manifest");
    validator
        .validate(&manifest)
        .expect("bundle manifest should validate");
}

#[test]
fn run_record_schema_accepts_success_shape() {
    let validator = load_schema("run-record.schema.json");
    let record = serde_json::json!({
        "program": "examples/hello.neb",
        "exit": 0,
        "diagnostics": [],
        "telemetry_path": "trace.jsonl",
        "probe_events": [
            {
                "ts": 1718380800,
                "probe": "log",
                "args": { "level": "info", "message": "ready" }
            }
        ],
        "duration_ms": 12,
        "printed": ["Hello from Nebula"],
        "return_value": null,
        "probes_called": [
            {
                "name": "log",
                "args": { "level": "info", "message": "ready" },
                "result": null
            }
        ]
    });
    validator
        .validate(&record)
        .expect("run record should validate");
}

#[test]
fn probe_manifest_schema_accepts_secrets_section() {
    let validator = load_schema("probe-manifest.schema.json");
    let manifest = serde_json::json!({
        "secrets": {
            "api_token": { "env": "OPENAI_API_KEY" },
            "dev_token": { "value": "sk-test" }
        },
        "probes": {
            "secret_get": { "kind": "secret_get" },
            "fetch": {
                "kind": "http_get",
                "headers": { "Authorization": "Bearer ${secret:api_token}" }
            }
        }
    });
    validator
        .validate(&manifest)
        .expect("secrets manifest should validate");
}

#[test]
fn nebula_value_schema_accepts_struct_and_option_wrappers() {
    let validator = load_schema("nebula-value.schema.json");
    let samples = [
        serde_json::json!(42),
        serde_json::json!(null),
        serde_json::json!({ "Some": "ready" }),
        serde_json::json!({
            "struct": "geo.Point",
            "fields": { "x": 1, "y": 2 }
        }),
        serde_json::json!({ "k": "v", "n": 1 }),
    ];

    for sample in samples {
        validator
            .validate(&sample)
            .unwrap_or_else(|_| panic!("value should validate: {sample}"));
    }
}
