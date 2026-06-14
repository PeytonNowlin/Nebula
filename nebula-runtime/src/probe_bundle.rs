use std::collections::HashMap;
use std::env;
use std::fs;

use crate::{RuntimeError, Value};

pub(crate) fn invoke_read_file(
    name: &str,
    args: &HashMap<String, Value>,
) -> Result<Value, RuntimeError> {
    let path = required_str_arg(name, args, "path")?;
    let content = fs::read_to_string(&path).map_err(|err| probe_failed(name, err.to_string()))?;
    Ok(Value::Str(content))
}

pub(crate) fn invoke_write_file(
    name: &str,
    args: &HashMap<String, Value>,
) -> Result<Value, RuntimeError> {
    let path = required_str_arg(name, args, "path")?;
    let content = required_str_arg(name, args, "content")?;
    fs::write(&path, content).map_err(|err| probe_failed(name, err.to_string()))?;
    Ok(Value::None)
}

pub(crate) fn invoke_http_get(
    name: &str,
    args: &HashMap<String, Value>,
    headers: Option<&HashMap<String, String>>,
) -> Result<Value, RuntimeError> {
    let url = required_str_arg(name, args, "url")?;
    let mut request = ureq::get(&url);
    if let Some(headers) = headers {
        for (key, value) in headers {
            request = request.header(key, value);
        }
    }
    let response = request
        .call()
        .map_err(|err| probe_failed(name, err.to_string()))?;
    let body = response
        .into_body()
        .read_to_string()
        .map_err(|err| probe_failed(name, err.to_string()))?;
    Ok(Value::Str(body))
}

pub(crate) fn invoke_json_parse(
    name: &str,
    args: &HashMap<String, Value>,
) -> Result<Value, RuntimeError> {
    let text = required_str_arg(name, args, "text")?;
    let value: serde_json::Value =
        serde_json::from_str(&text).map_err(|err| probe_failed(name, err.to_string()))?;
    let serde_json::Value::Object(map) = value else {
        return Err(probe_failed(
            name,
            "json_parse requires a JSON object at the top level".into(),
        ));
    };
    let mut fields = HashMap::new();
    for (key, json) in map {
        fields.insert(key, json_to_value(json)?);
    }
    Ok(Value::Map(fields))
}

pub(crate) fn invoke_env_get(
    name: &str,
    args: &HashMap<String, Value>,
) -> Result<Value, RuntimeError> {
    let key = required_str_arg(name, args, "key")?;
    match env::var(&key) {
        Ok(value) => Ok(Value::Some(Box::new(Value::Str(value)))),
        Err(env::VarError::NotPresent) => Ok(Value::None),
        Err(env::VarError::NotUnicode(_)) => Err(probe_failed(
            name,
            format!("environment variable `{key}` is not valid UTF-8"),
        )),
    }
}

pub(crate) fn invoke_secret_get(
    name: &str,
    args: &HashMap<String, Value>,
    secrets: &HashMap<String, String>,
) -> Result<Value, RuntimeError> {
    let key = required_str_arg(name, args, "name")?;
    match secrets.get(&key) {
        Some(value) => Ok(Value::Some(Box::new(Value::Str(value.clone())))),
        None => Ok(Value::None),
    }
}

fn required_str_arg(
    probe_name: &str,
    args: &HashMap<String, Value>,
    name: &str,
) -> Result<String, RuntimeError> {
    match args.get(name) {
        Some(Value::Str(s)) => Ok(s.clone()),
        Some(_) => Err(probe_failed(
            probe_name,
            format!("argument `{name}` must be Str"),
        )),
        None => Err(probe_failed(
            probe_name,
            format!("missing required argument `{name}`"),
        )),
    }
}

fn probe_failed(name: &str, message: String) -> RuntimeError {
    RuntimeError::ProbeFailed {
        name: name.to_string(),
        message,
        span: 0..0,
    }
}

fn json_to_value(value: serde_json::Value) -> Result<Value, RuntimeError> {
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
                    message: "unsupported JSON number in parsed value".into(),
                    span: 0..0,
                });
            }
        }
        serde_json::Value::String(s) => Value::Str(s),
        serde_json::Value::Array(items) => Value::List(
            items
                .into_iter()
                .map(json_to_value)
                .collect::<Result<_, _>>()?,
        ),
        serde_json::Value::Object(map) => Value::Map(
            map.into_iter()
                .map(|(k, v)| json_to_value(v).map(|val| (k, val)))
                .collect::<Result<_, _>>()?,
        ),
    })
}
