use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};

use nebula_mcp::{McpConnectionManager, McpServerConfig, McpToolDescriptor};
use serde::{Deserialize, Serialize};

use crate::RuntimeError;

#[derive(Debug, Clone, Deserialize)]
pub struct ProbeManifest {
    #[serde(default)]
    pub mcp_servers: HashMap<String, McpServerConfig>,
    pub probes: HashMap<String, ProbeBinding>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum ProbeBinding {
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
    ReadFile,
    WriteFile,
    HttpGet,
    JsonParse,
    EnvGet,
}

#[derive(Debug, Clone, Serialize)]
pub struct ProbeListReport {
    pub manifest: String,
    pub probes: Vec<DeclaredProbe>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub mcp_servers: Option<HashMap<String, McpServerReport>>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum DeclaredProbe {
    Jsonl {
        name: String,
        #[serde(skip_serializing_if = "Option::is_none")]
        path: Option<String>,
    },
    Command {
        name: String,
        command: Vec<String>,
    },
    Mcp {
        name: String,
        server: String,
        #[serde(skip_serializing_if = "Option::is_none")]
        tool: Option<String>,
    },
    ReadFile {
        name: String,
    },
    WriteFile {
        name: String,
    },
    HttpGet {
        name: String,
    },
    JsonParse {
        name: String,
    },
    EnvGet {
        name: String,
    },
}

#[derive(Debug, Clone, Serialize)]
pub struct McpServerReport {
    pub transport: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tools: Option<Vec<McpToolDescriptor>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

pub fn read_probe_manifest(path: &Path) -> Result<ProbeManifest, RuntimeError> {
    let source = fs::read_to_string(path).map_err(|err| RuntimeError::Error {
        message: format!("failed to read probe manifest `{}`: {err}", path.display()),
    })?;
    let mut manifest: ProbeManifest = serde_json::from_str(&source).map_err(|err| {
        RuntimeError::Error {
            message: format!("invalid probe manifest `{}`: {err}", path.display()),
        }
    })?;
    resolve_manifest_paths(&mut manifest, path);
    Ok(manifest)
}

fn resolve_manifest_paths(manifest: &mut ProbeManifest, manifest_path: &Path) {
    for config in manifest.mcp_servers.values_mut() {
        if matches!(config.transport, nebula_mcp::McpTransportKind::Stdio) {
            for arg in &mut config.command {
                resolve_relative_path_arg(arg, manifest_path);
            }
        }
    }

    for binding in manifest.probes.values_mut() {
        match binding {
            ProbeBinding::Jsonl { path } => {
                if let Some(path) = path {
                    *path = resolve_relative_path(path, manifest_path);
                }
            }
            ProbeBinding::Command { command } => {
                for arg in command {
                    resolve_relative_path_arg(arg, manifest_path);
                }
            }
            ProbeBinding::Mcp { .. }
            | ProbeBinding::ReadFile
            | ProbeBinding::WriteFile
            | ProbeBinding::HttpGet
            | ProbeBinding::JsonParse
            | ProbeBinding::EnvGet => {}
        }
    }
}

fn resolve_relative_path_arg(arg: &mut String, manifest_path: &Path) {
    let candidate = Path::new(arg.as_str());
    if candidate.is_absolute() {
        return;
    }
    if !(arg.contains('/') || arg.contains('\\') || arg.starts_with('.')) {
        return;
    }
    *arg = resolve_relative_path(candidate, manifest_path)
        .display()
        .to_string();
}

fn resolve_relative_path(path: &Path, manifest_path: &Path) -> PathBuf {
    if path.is_absolute() {
        return path.to_path_buf();
    }

    let manifest_dir = manifest_path
        .parent()
        .unwrap_or_else(|| Path::new("."));
    let candidates = [
        manifest_dir.join(path),
        manifest_dir
            .parent()
            .map(|parent| parent.join(path))
            .unwrap_or_else(|| manifest_dir.join(path)),
        PathBuf::from(path),
    ];

    for candidate in &candidates {
        if candidate.exists() {
            return candidate.canonicalize().unwrap_or_else(|_| candidate.clone());
        }
    }

    candidates
        .into_iter()
        .find(|candidate| candidate.parent().is_some_and(|parent| parent.exists()))
        .unwrap_or_else(|| manifest_dir.join(path))
}

pub fn list_probe_manifest(path: &Path, discover_mcp: bool) -> Result<ProbeListReport, RuntimeError> {
    let manifest = read_probe_manifest(path)?;
    validate_manifest(&manifest)?;

    let mut probes = Vec::new();
    for (name, binding) in &manifest.probes {
        probes.push(declared_probe(name, binding));
    }
    probes.sort_by(|left, right| probe_name(left).cmp(probe_name(right)));

    let mcp_servers = if discover_mcp && !manifest.mcp_servers.is_empty() {
        Some(discover_mcp_servers(&manifest)?)
    } else {
        None
    };

    Ok(ProbeListReport {
        manifest: path.display().to_string(),
        probes,
        mcp_servers,
    })
}

fn probe_name(probe: &DeclaredProbe) -> &str {
    match probe {
        DeclaredProbe::Jsonl { name, .. }
        | DeclaredProbe::Command { name, .. }
        | DeclaredProbe::Mcp { name, .. }
        | DeclaredProbe::ReadFile { name }
        | DeclaredProbe::WriteFile { name }
        | DeclaredProbe::HttpGet { name }
        | DeclaredProbe::JsonParse { name }
        | DeclaredProbe::EnvGet { name } => name,
    }
}

fn declared_probe(name: &str, binding: &ProbeBinding) -> DeclaredProbe {
    match binding {
        ProbeBinding::Jsonl { path } => DeclaredProbe::Jsonl {
            name: name.to_string(),
            path: path.as_ref().map(|p| p.display().to_string()),
        },
        ProbeBinding::Command { command } => DeclaredProbe::Command {
            name: name.to_string(),
            command: command.clone(),
        },
        ProbeBinding::Mcp { server, tool } => DeclaredProbe::Mcp {
            name: name.to_string(),
            server: server.clone(),
            tool: tool.clone(),
        },
        ProbeBinding::ReadFile => DeclaredProbe::ReadFile {
            name: name.to_string(),
        },
        ProbeBinding::WriteFile => DeclaredProbe::WriteFile {
            name: name.to_string(),
        },
        ProbeBinding::HttpGet => DeclaredProbe::HttpGet {
            name: name.to_string(),
        },
        ProbeBinding::JsonParse => DeclaredProbe::JsonParse {
            name: name.to_string(),
        },
        ProbeBinding::EnvGet => DeclaredProbe::EnvGet {
            name: name.to_string(),
        },
    }
}

pub fn validate_manifest(manifest: &ProbeManifest) -> Result<(), RuntimeError> {
    for (name, binding) in &manifest.probes {
        if let ProbeBinding::Mcp { server, .. } = binding {
            if manifest.mcp_servers.is_empty() {
                return Err(RuntimeError::Error {
                    message: format!(
                        "probe `{name}` uses kind mcp but manifest defines no mcp_servers"
                    ),
                });
            }
            if !manifest.mcp_servers.contains_key(server) {
                return Err(RuntimeError::Error {
                    message: format!("probe `{name}` references unknown MCP server `{server}`"),
                });
            }
        }
    }
    Ok(())
}

fn discover_mcp_servers(
    manifest: &ProbeManifest,
) -> Result<HashMap<String, McpServerReport>, RuntimeError> {
    let manager = McpConnectionManager::new(manifest.mcp_servers.clone())
        .map_err(|err| RuntimeError::Error {
            message: err.to_string(),
        })?;

    let mut reports = HashMap::new();
    for (server_id, config) in &manifest.mcp_servers {
        let transport = match config.transport {
            nebula_mcp::McpTransportKind::Stdio => "stdio",
            nebula_mcp::McpTransportKind::Http => "http",
        }
        .to_string();

        match manager.list_tools(server_id) {
            Ok(mut tools) => {
                tools.sort_by(|left, right| left.name.cmp(&right.name));
                reports.insert(
                    server_id.clone(),
                    McpServerReport {
                        transport,
                        tools: Some(tools),
                        error: None,
                    },
                );
            }
            Err(err) => {
                reports.insert(
                    server_id.clone(),
                    McpServerReport {
                        transport,
                        tools: None,
                        error: Some(err.to_string()),
                    },
                );
            }
        }
    }

    Ok(reports)
}