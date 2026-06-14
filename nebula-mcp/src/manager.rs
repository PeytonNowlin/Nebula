use std::collections::HashMap;
use std::sync::Mutex;

use serde_json::{Map, Value};

use crate::config::{McpServerConfig, McpTransportKind};
use crate::error::McpError;
use crate::session::McpSession;
use crate::transport::{HttpMcpSession, StdioMcpSession};

enum SessionKind {
    Stdio(StdioMcpSession),
    Http(HttpMcpSession),
}

impl McpSession for SessionKind {
    fn call_tool(&self, tool: &str, arguments: Value) -> Result<(), McpError> {
        match self {
            SessionKind::Stdio(session) => session.call_tool(tool, arguments),
            SessionKind::Http(session) => session.call_tool(tool, arguments),
        }
    }
}

pub struct McpConnectionManager {
    servers: HashMap<String, McpServerConfig>,
    sessions: Mutex<HashMap<String, SessionKind>>,
}

impl McpConnectionManager {
    pub fn new(servers: HashMap<String, McpServerConfig>) -> Result<Self, McpError> {
        for (server_id, config) in &servers {
            config.validate(server_id)?;
        }
        Ok(Self {
            servers,
            sessions: Mutex::new(HashMap::new()),
        })
    }

    pub fn call_tool(
        &self,
        server_id: &str,
        tool: &str,
        arguments: HashMap<String, Value>,
    ) -> Result<(), McpError> {
        let config = self.servers.get(server_id).ok_or_else(|| {
            McpError::config(format!("unknown MCP server `{server_id}` in probe manifest"))
        })?;
        let args = Value::Object(arguments.into_iter().collect::<Map<String, Value>>());

        let mut sessions = self.sessions.lock().map_err(|_| {
            McpError::transport("MCP connection manager lock poisoned")
        })?;

        if !sessions.contains_key(server_id) {
            let session = match config.transport {
                McpTransportKind::Stdio => SessionKind::Stdio(StdioMcpSession::connect(config)?),
                McpTransportKind::Http => SessionKind::Http(HttpMcpSession::connect(config)?),
            };
            sessions.insert(server_id.to_string(), session);
        }

        let session = sessions
            .get(server_id)
            .expect("session inserted above if missing");
        session.call_tool(tool, args)
    }
}
