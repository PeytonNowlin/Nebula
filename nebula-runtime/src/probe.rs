use std::collections::HashMap;
use std::fs::OpenOptions;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::{SystemTime, UNIX_EPOCH};

use nebula_mcp::{McpConnectionManager, McpError};
use serde::{Deserialize, Serialize};

use crate::probe_bundle::{
    invoke_env_get, invoke_http_get, invoke_json_parse, invoke_read_file, invoke_secret_get,
    invoke_write_file,
};
use crate::probe_manifest::{
    prepare_probe_manifest, read_probe_manifest, validate_manifest, ProbeBinding,
};
use crate::secrets::SecretsStore;
use crate::value_json::{json_to_value, value_to_json};
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

/// One JSONL-style record emitted by `jsonl` probe handlers (including built-in `log`).
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ProbeJsonlEvent {
    pub ts: u64,
    pub probe: String,
    pub args: HashMap<String, serde_json::Value>,
}

#[derive(Debug, Clone)]
enum Handler {
    Jsonl {
        path: Option<PathBuf>,
    },
    Command {
        command: Vec<String>,
        env: HashMap<String, String>,
    },
    Mcp {
        server: String,
        tool: Option<String>,
    },
    ReadFile,
    WriteFile,
    HttpGet {
        headers: HashMap<String, String>,
    },
    JsonParse,
    EnvGet,
    SecretGet,
}

/// Default probe host: built-in handlers plus optional manifest overrides.
pub struct RegistryProbeHost {
    handlers: HashMap<String, Handler>,
    mcp_manager: Option<McpConnectionManager>,
    secrets: SecretsStore,
    probe_events: Vec<ProbeJsonlEvent>,
}

impl RegistryProbeHost {
    pub fn with_defaults() -> Self {
        let mut handlers = HashMap::new();
        handlers.insert("log".into(), Handler::Jsonl { path: None });
        Self {
            handlers,
            mcp_manager: None,
            secrets: SecretsStore::new(),
            probe_events: Vec::new(),
        }
    }

    pub fn take_probe_events(&mut self) -> Vec<ProbeJsonlEvent> {
        std::mem::take(&mut self.probe_events)
    }

    pub fn load_manifest(
        &mut self,
        path: &Path,
        secrets_overlay: Option<&SecretsStore>,
    ) -> Result<(), RuntimeError> {
        let manifest = read_probe_manifest(path)?;
        validate_manifest(&manifest)?;
        let (manifest, secrets) = prepare_probe_manifest(manifest, secrets_overlay)?;
        self.secrets = secrets;

        if !manifest.mcp_servers.is_empty() {
            self.mcp_manager = Some(
                McpConnectionManager::new(manifest.mcp_servers.clone())
                    .map_err(mcp_error_to_runtime)?,
            );
        } else {
            self.mcp_manager = None;
        }

        for (name, binding) in manifest.probes {
            self.handlers.insert(name, binding.into());
        }
        Ok(())
    }

    fn handler_for(&self, name: &str) -> Option<&Handler> {
        self.handlers.get(name).or_else(|| {
            name.rsplit('.')
                .next()
                .and_then(|short| self.handlers.get(short))
        })
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
        let handler = self
            .handler_for(call.name)
            .ok_or(RuntimeError::ProbeNotImplemented {
                name: call.name.to_string(),
                span: 0..0,
            })?
            .clone();

        match handler {
            Handler::Jsonl { path } => {
                invoke_jsonl_log(call, path.as_deref(), &mut self.probe_events)
            }
            Handler::Command { command, env } => invoke_command_probe(call, &command, &env),
            Handler::Mcp { server, tool } => {
                let manager = self
                    .mcp_manager
                    .as_ref()
                    .ok_or_else(|| RuntimeError::Error {
                        message: format!(
                            "probe `{}` is configured as MCP but no MCP servers are loaded",
                            call.name
                        ),
                        span: 0..0,
                    })?;
                let tool_name = Self::resolve_tool_name(call.name, &tool);
                let args = call
                    .args
                    .iter()
                    .map(|(k, v)| (k.clone(), value_to_json(v)))
                    .collect();
                manager
                    .call_tool(&server, &tool_name, args)
                    .map_err(|err| mcp_invoke_error(call.name, err))?;
                Ok(Value::None)
            }
            Handler::ReadFile => invoke_read_file(call.name, &call.args),
            Handler::WriteFile => invoke_write_file(call.name, &call.args),
            Handler::HttpGet { headers } => {
                let header_ref = if headers.is_empty() {
                    None
                } else {
                    Some(&headers)
                };
                invoke_http_get(call.name, &call.args, header_ref)
            }
            Handler::JsonParse => invoke_json_parse(call.name, &call.args),
            Handler::EnvGet => invoke_env_get(call.name, &call.args),
            Handler::SecretGet => invoke_secret_get(call.name, &call.args, &self.secrets),
        }
    }
}

fn mcp_error_to_runtime(err: McpError) -> RuntimeError {
    match err {
        McpError::Transport { message } => RuntimeError::McpTransport {
            message,
            span: 0..0,
        },
        McpError::ToolFailed { tool, message } => RuntimeError::ProbeFailed {
            name: tool,
            message,
            span: 0..0,
        },
        McpError::Config { message } => RuntimeError::Error {
            message,
            span: 0..0,
        },
    }
}

fn mcp_invoke_error(probe_name: &str, err: McpError) -> RuntimeError {
    match err {
        McpError::Transport { message } => RuntimeError::McpTransport {
            message,
            span: 0..0,
        },
        McpError::ToolFailed { .. } => RuntimeError::ProbeFailed {
            name: probe_name.to_string(),
            message: err.to_string(),
            span: 0..0,
        },
        McpError::Config { message } => RuntimeError::Error {
            message,
            span: 0..0,
        },
    }
}

fn invoke_jsonl_log(
    call: &ProbeInvocation<'_>,
    path: Option<&Path>,
    events: &mut Vec<ProbeJsonlEvent>,
) -> Result<Value, RuntimeError> {
    let redact_args = call
        .name
        .rsplit('.')
        .next()
        .is_some_and(|short| short == "secret_get");
    let event = ProbeJsonlEvent {
        ts: SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_secs())
            .unwrap_or(0),
        probe: call.name.to_string(),
        args: call
            .args
            .iter()
            .map(|(k, v)| {
                if redact_args {
                    (k.clone(), serde_json::Value::String("<redacted>".into()))
                } else {
                    (k.clone(), value_to_json(v))
                }
            })
            .collect(),
    };
    events.push(event.clone());
    let line = serde_json::to_string(&event).map_err(|err| RuntimeError::ProbeFailed {
        name: call.name.to_string(),
        message: err.to_string(),
        span: 0..0,
    })?;

    if let Some(path) = path {
        let mut file = OpenOptions::new()
            .create(true)
            .append(true)
            .open(path)
            .map_err(|err| RuntimeError::ProbeFailed {
                name: call.name.to_string(),
                message: format!("failed to open probe log `{}`: {err}", path.display()),
                span: 0..0,
            })?;
        writeln!(file, "{line}").map_err(|err| RuntimeError::ProbeFailed {
            name: call.name.to_string(),
            message: format!("failed to write probe log: {err}"),
            span: 0..0,
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
    env: &HashMap<String, String>,
) -> Result<Value, RuntimeError> {
    if command.is_empty() {
        return Err(RuntimeError::ProbeFailed {
            name: call.name.to_string(),
            message: "command probe requires a non-empty command".into(),
            span: 0..0,
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
    let request_json =
        serde_json::to_string(&request).map_err(|err| RuntimeError::ProbeFailed {
            name: call.name.to_string(),
            message: err.to_string(),
            span: 0..0,
        })?;

    let mut child_cmd = Command::new(&command[0]);
    child_cmd
        .args(&command[1..])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null());
    if !env.is_empty() {
        child_cmd.envs(env);
    }
    let mut child = child_cmd.spawn().map_err(|err| RuntimeError::ProbeFailed {
        name: call.name.to_string(),
        message: format!("failed to spawn probe command: {err}"),
        span: 0..0,
    })?;

    if let Some(mut stdin) = child.stdin.take() {
        stdin
            .write_all(request_json.as_bytes())
            .map_err(|err| RuntimeError::ProbeFailed {
                name: call.name.to_string(),
                message: format!("failed to write probe request: {err}"),
                span: 0..0,
            })?;
    }

    let output = child
        .wait_with_output()
        .map_err(|err| RuntimeError::ProbeFailed {
            name: call.name.to_string(),
            message: format!("failed to wait for probe command: {err}"),
            span: 0..0,
        })?;

    if !output.status.success() {
        return Err(RuntimeError::ProbeFailed {
            name: call.name.to_string(),
            message: format!("probe command exited with status {}", output.status),
            span: 0..0,
        });
    }

    let response: CommandResponse =
        serde_json::from_slice(&output.stdout).map_err(|err| RuntimeError::ProbeFailed {
            name: call.name.to_string(),
            message: format!("invalid probe response JSON: {err}"),
            span: 0..0,
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
            span: 0..0,
        }),
    }
}

impl From<ProbeBinding> for Handler {
    fn from(binding: ProbeBinding) -> Self {
        match binding {
            ProbeBinding::Jsonl { path } => Handler::Jsonl { path },
            ProbeBinding::Command { command, env } => Handler::Command { command, env },
            ProbeBinding::Mcp { server, tool } => Handler::Mcp { server, tool },
            ProbeBinding::ReadFile => Handler::ReadFile,
            ProbeBinding::WriteFile => Handler::WriteFile,
            ProbeBinding::HttpGet { headers } => Handler::HttpGet { headers },
            ProbeBinding::JsonParse => Handler::JsonParse,
            ProbeBinding::EnvGet => Handler::EnvGet,
            ProbeBinding::SecretGet => Handler::SecretGet,
        }
    }
}
