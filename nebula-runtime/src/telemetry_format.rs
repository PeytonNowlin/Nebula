use std::collections::HashMap;

use serde::Serialize;
use serde_json::{json, Value as JsonValue};

use crate::value_json::value_to_json;
use crate::Value;

/// One probe invocation recorded for run output (`probes_called`).
#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct ProbeCallRecord {
    pub name: String,
    pub args: HashMap<String, JsonValue>,
    pub result: JsonValue,
}

pub fn probe_call_record(
    name: &str,
    args: &HashMap<String, Value>,
    result: &Value,
) -> ProbeCallRecord {
    let short = name.rsplit('.').next().unwrap_or(name);
    let redact = short == "secret_get";
    ProbeCallRecord {
        name: name.to_string(),
        args: probe_args(args, redact),
        result: probe_result_summary(name, result),
    }
}

const STR_PREVIEW_MAX: usize = 256;

#[derive(Serialize)]
pub struct TelemetryEvent {
    pub step: String,
    pub detail: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub value: Option<JsonValue>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub args: Option<HashMap<String, JsonValue>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub result: Option<JsonValue>,
}

pub fn binding_value(value: &Value) -> JsonValue {
    value_to_json(value)
}

pub fn probe_args(args: &HashMap<String, Value>, redact_secret_names: bool) -> HashMap<String, JsonValue> {
    args.iter()
        .map(|(key, value)| {
            let json = if redact_secret_names {
                JsonValue::String("<redacted>".into())
            } else {
                value_to_json(value)
            };
            (key.clone(), json)
        })
        .collect()
}

pub fn probe_result_summary(probe_name: &str, value: &Value) -> JsonValue {
    let short = probe_name.rsplit('.').next().unwrap_or(probe_name);
    if short == "secret_get" {
        return json!({ "_summary": "Str", "redacted": true });
    }
    summarize_value(value)
}

fn summarize_value(value: &Value) -> JsonValue {
    match value {
        Value::None => JsonValue::Null,
        Value::Int(_) | Value::Float(_) | Value::Bool(_) => value_to_json(value),
        Value::Str(s) => summarize_string(s),
        Value::Some(inner) => json!({
            "_summary": "Some",
            "inner": summarize_value(inner),
        }),
        Value::List(items) => json!({
            "_summary": "List",
            "len": items.len(),
        }),
        Value::Map(map) => json!({
            "_summary": "Map",
            "len": map.len(),
        }),
        Value::Struct { name, fields } => json!({
            "_summary": "Struct",
            "name": name,
            "fields": fields.len(),
        }),
    }
}

fn summarize_string(value: &str) -> JsonValue {
    if value.len() <= STR_PREVIEW_MAX {
        return JsonValue::String(value.to_string());
    }
    json!({
        "_summary": "Str",
        "len": value.len(),
        "preview": &value[..STR_PREVIEW_MAX],
    })
}