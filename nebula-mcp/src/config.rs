use std::collections::HashMap;

use serde::Deserialize;

#[derive(Debug, Clone, Deserialize)]
pub struct McpServerConfig {
    pub transport: McpTransportKind,
    #[serde(default)]
    pub command: Vec<String>,
    #[serde(default)]
    pub env: HashMap<String, String>,
    #[serde(default)]
    pub url: Option<String>,
    #[serde(default)]
    pub headers: HashMap<String, String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum McpTransportKind {
    Stdio,
    Http,
}

impl McpServerConfig {
    pub fn validate(&self, server_id: &str) -> Result<(), crate::McpError> {
        match self.transport {
            McpTransportKind::Stdio => {
                if self.command.is_empty() {
                    return Err(crate::McpError::config(format!(
                        "mcp_servers.{server_id} requires non-empty `command` for stdio transport"
                    )));
                }
            }
            McpTransportKind::Http => {
                if self.url.as_ref().is_none_or(|u| u.is_empty()) {
                    return Err(crate::McpError::config(format!(
                        "mcp_servers.{server_id} requires `url` for http transport"
                    )));
                }
            }
        }
        Ok(())
    }
}
