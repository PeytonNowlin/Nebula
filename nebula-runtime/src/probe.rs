use std::collections::HashMap;
use std::fs::{self, OpenOptions};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::{SystemTime, UNIX_EPOCH};

use nebula_mcp::{McpConnectionManager, McpError, McpServerConfig};
use serde::{Deserialize, Serialize};

use crate::{RuntimeError, Value};

/// A probe invocation dispatched to the host.
pub struct ProbeInvocation<'a> {
    pub name: &'a str,
    pub args: HashMap<String, Value>,
}

/// Host-side probe dispatch. Probes declare capabilities in source; the host provides implementations.
pub trait ProbeHost {
    fn invoke(&mut self, call: &ProbeInvocation<'_>) -> Result<Value, RuntimeError>;
}

#[derive(Debug, Clone, Serialize)]
struct ProbeEvent {
    ts: u64,
    probe: String,
    args: HashMap<String, serde_json::Value>,
}

#[derive(Debug, Clone, Deserialize)]
struct ProbeManifest {
    #[serde(default)]
    mcp_servers: HashMap<String, McpServerConfig>,
    probes: HashMap<String, ProbeBinding>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
enum ProbeBinding {
    Jsonl {
        #[serde(default)]
        path: Option<PathBuf>,
    },
    Command {
        command: Vec<String>,
    },
    Mcp {
        server: String,
        #[serde(default)]
        tool: Option<String>,
    },
}

#[derive(Debug, Clone)]
enum Handler {
    Jsonl { path: Option<PathBuf> },
    Command { command: Vec<String> },
    Mcp {
        server: String,
        tool: Option<String>,
    },
}

/// Default probe host: built-in handlers plus optional manifest overrides.
pub struct RegistryProbeHost {
    handlers: HashMap<String, Handler>,
    mcp_manager: Option<McpConnectionManager>,
}

impl RegistryProbeHost {
    pub fn with_defaults() -> Self {
        let mut handlers = HashMap::new();
        handlers.insert(
            "log".into(),
            Handler::Jsonl {
                path: None,
            },
        );
        Self {
            handlers,
            mcp_manager: None,
        }
    }

    pub fn load_manifest(&mut self, path: &Path) -> Result<(), RuntimeError> {
        let source = fs::read_to_string(path).map_err(|err| RuntimeError::Error {
            message: format!("failed to read probe manifest `{}`: {err}", path.display()),
        })?;
        let manifest: ProbeManifest = serde_json::from_str(&source).map_err(|err| {
            RuntimeError::Error {
                message: format!("invalid probe manifest `{}`: {err}", path.display()),
            }
        })?;

        if !manifest.mcp_servers.is_empty() {
            self.mcp_manager = Some(
                McpConnectionManager::new(manifest.mcp_servers.clone())
                    .map_err(mcp_error_to_runtime)?,
            );
        } else {
            self.mcp_manager = None;
        }

        for (name, binding) in manifest.probes {
            if let ProbeBinding::Mcp { server, .. } = &binding {
                if self.mcp_manager.is_none() {
                    return Err(RuntimeError::Error {
                        message: format!(
                            "probe `{name}` uses kind mcp but manifest defines no mcp_servers"
                        ),
                    });
                }
                if !manifest.mcp_servers.contains_key(server) {
                    return Err(RuntimeError::Error {
                        message: format!(
                            "probe `{name}` references unknown MCP server `{server}`"
                        ),
                    });
                }
            }
            self.handlers.insert(name, binding.into());
        }
        Ok(())
    }

    fn handler_for(&self, name: &str) -> Option<&Handler> {
        self.handlers
            .get(name)
            .or_else(|| name.rsplit('.').next().and_then(|short| self.handlers.get(short)))
    }

    fn resolve_tool_name(probe_name: &str, tool: &Option<String>) -> String {
        tool.clone().unwrap_or_else(|| {
            probe_name
                .rsplit('.')
                .next()
                .unwrap_or(probe_name)
                .to_string()
        })
    }
}

impl ProbeHost for RegistryProbeHost {
    fn invoke(&mut self, call: &ProbeInvocation<'_>) -> Result<Value, RuntimeError> {
        let handler = self.handler_for(call.name).ok_or(RuntimeError::ProbeNotImplemented {
            name: call.name.to_string(),
        })?;

        match handler {
            Handler::Jsonl { path } => invoke_jsonl_log(call, path.as_deref()),
            Handler::Command { command } => invoke_command_probe(call, command),
            Handler::Mcp { server, tool } => {
                let manager = self.mcp_manager.as_ref().ok_or_else(|| RuntimeError::Error {
                    message: format!(
                        "probe `{}` is configured as MCP but no MCP servers are loaded",
                        call.name
                    ),
                })?;
                let tool_name = Self::resolve_tool_name(call.name, tool);
                let args = call
                    .args
                    .iter()
                    .map(|(k, v)| (k.clone(), value_to_json(v)))
                    .collect();
                manager
                    .call_tool(server, &tool_name, args)
                    .map_err(|err| mcp_invoke_error(call.name, err))?;
                Ok(Value::None)
            }
        }
    }
}

fn mcp_error_to_runtime(err: McpError) -> RuntimeError {
    match err {
        McpError::Transport { message } => RuntimeError::McpTransport { message },
        McpError::ToolFailed { tool, message } => RuntimeError::ProbeFailed {
            name: tool,
            message,
        },
        McpError::Config { message } => RuntimeError::Error { message },
    }
}

fn mcp_invoke_error(probe_name: &str, err: McpError) -> RuntimeError {
    match err {
        McpError::Transport { message } => RuntimeError::McpTransport { message },
        McpError::ToolFailed { .. } => RuntimeError::ProbeFailed {
            name: probe_name.to_string(),
            message: err.to_string(),
        },
        McpError::Config { message } => RuntimeError::Error { message },
    }
}

fn invoke_jsonl_log(
    call: &ProbeInvocation<'_>,
    path: Option<&Path>,
) -> Result<Value, RuntimeError> {
    let event = ProbeEvent {
        ts: SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_secs())
            .unwrap_or(0),
        probe: call.name.to_string(),
        args: call
            .args
            .iter()
            .map(|(k, v)| (k.clone(), value_to_json(v)))
            .collect(),
    };
    let line = serde_json::to_string(&event).map_err(|err| RuntimeError::ProbeFailed {
        name: call.name.to_string(),
        message: err.to_string(),
    })?;

    if let Some(path) = path {
        let mut file = OpenOptions::new()
            .create(true)
            .append(true)
            .open(path)
            .map_err(|err| RuntimeError::ProbeFailed {
                name: call.name.to_string(),
                message: format!("failed to open probe log `{}`: {err}", path.display()),
            })?;
        writeln!(file, "{line}").map_err(|err| RuntimeError::ProbeFailed {
            name: call.name.to_string(),
            message: format!("failed to write probe log: {err}"),
        })?;
    } else {
        eprintln!("{line}");
    }

    Ok(Value::None)
}

#[derive(Debug, Serialize, Deserialize)]
struct CommandRequest<'a> {
    probe: &'a str,
    args: HashMap<String, serde_json::Value>,
}

#[derive(Debug, Deserialize)]
struct CommandResponse {
    status: String,
    #[serde(default)]
    value: Option<serde_json::Value>,
    #[serde(default)]
    message: Option<String>,
}

fn invoke_command_probe(
    call: &ProbeInvocation<'_>,
    command: &[String],
) -> Result<Value, RuntimeError> {
    if command.is_empty() {
        return Err(RuntimeError::ProbeFailed {
            name: call.name.to_string(),
            message: "command probe requires a non-empty command".into(),
        });
    }

    let request = CommandRequest {
        probe: call.name,
        args: call
            .args
            .iter()
            .map(|(k, v)| (k.clone(), value_to_json(v)))
            .collect(),
    };
    let request_json = serde_json::to_string(&request).map_err(|err| RuntimeError::ProbeFailed {
        name: call.name.to_string(),
        message: err.to_string(),
    })?;

    let mut child = Command::new(&command[0])
        .args(&command[1..])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .map_err(|err| RuntimeError::ProbeFailed {
            name: call.name.to_string(),
            message: format!("failed to spawn probe command: {err}"),
        })?;

    if let Some(mut stdin) = child.stdin.take() {
        stdin
            .write_all(request_json.as_bytes())
            .map_err(|err| RuntimeError::ProbeFailed {
                name: call.name.to_string(),
                message: format!("failed to write probe request: {err}"),
            })?;
    }

    let output = child.wait_with_output().map_err(|err| RuntimeError::ProbeFailed {
        name: call.name.to_string(),
        message: format!("failed to wait for probe command: {err}"),
    })?;

    if !output.status.success() {
        return Err(RuntimeError::ProbeFailed {
            name: call.name.to_string(),
            message: format!(
                "probe command exited with status {}",
                output.status
            ),
        });
    }

    let response: CommandResponse =
        serde_json::from_slice(&output.stdout).map_err(|err| RuntimeError::ProbeFailed {
            name: call.name.to_string(),
            message: format!("invalid probe response JSON: {err}"),
        })?;

    match response.status.as_str() {
        "ok" => match response.value {
            Some(value) => json_to_value(value),
            None => Ok(Value::None),
        },
        _ => Err(RuntimeError::ProbeFailed {
            name: call.name.to_string(),
            message: response
                .message
                .unwrap_or_else(|| "probe command returned error status".into()),
        }),
    }
}

impl From<ProbeBinding> for Handler {
    fn from(binding: ProbeBinding) -> Self {
        match binding {
            ProbeBinding::Jsonl { path } => Handler::Jsonl { path },
            ProbeBinding::Command { command } => Handler::Command { command },
            ProbeBinding::Mcp { server, tool } => Handler::Mcp { server, tool },
        }
    }
}

fn value_to_json(value: &Value) -> serde_json::Value {
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
                    message: "unsupported JSON number in probe response".into(),
                });
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
