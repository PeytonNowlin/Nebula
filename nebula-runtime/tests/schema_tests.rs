use std::collections::HashMap;
use std::fs;
use std::path::PathBuf;

use jsonschema::{Resource, Retrieve, Uri, Validator};
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
    let sample_path = workspace_root().join("telemetry.jsonl");
    let source = fs::read_to_string(&sample_path).expect("read telemetry.jsonl");

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