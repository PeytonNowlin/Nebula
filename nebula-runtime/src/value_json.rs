use std::collections::HashMap;

use crate::{RuntimeError, Value};

pub fn value_to_json(value: &Value) -> serde_json::Value {
    match value {
        Value::Int(n) => serde_json::Value::from(*n),
        Value::Float(n) => serde_json::json!(*n),
        Value::Bool(b) => serde_json::Value::from(*b),
        Value::Str(s) => serde_json::Value::from(s.clone()),
        Value::None => serde_json::Value::Null,
        Value::Some(inner) => serde_json::json!({ "Some": value_to_json(inner) }),
        Value::List(items) => {
            serde_json::Value::Array(items.iter().map(value_to_json).collect())
        }
        Value::Map(map) => {
            let obj = map
                .iter()
                .map(|(k, v)| (k.clone(), value_to_json(v)))
                .collect();
            serde_json::Value::Object(obj)
        }
        Value::Struct { name, fields } => serde_json::json!({
            "struct": name,
            "fields": fields.iter().map(|(k, v)| (k, value_to_json(v))).collect::<HashMap<_, _>>(),
        }),
    }
}

pub fn json_to_value(value: serde_json::Value) -> Result<Value, RuntimeError> {
    Ok(match value {
        serde_json::Value::Null => Value::None,
        serde_json::Value::Bool(b) => Value::Bool(b),
        serde_json::Value::Number(n) => {
            if let Some(i) = n.as_i64() {
                Value::Int(i)
            } else if let Some(f) = n.as_f64() {
                Value::Float(f)
            } else {
                return Err(RuntimeError::Error {
                    message: "unsupported JSON number in value".into(), span: 0..0 });
            }
        }
        serde_json::Value::String(s) => Value::Str(s),
        serde_json::Value::Array(items) => {
            Value::List(items.into_iter().map(json_to_value).collect::<Result<_, _>>()?)
        }
        serde_json::Value::Object(map) => Value::Map(
            map.into_iter()
                .map(|(k, v)| json_to_value(v).map(|val| (k, val)))
                .collect::<Result<_, _>>()?,
        ),
    })
}