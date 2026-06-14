use std::io::{BufRead, BufReader, Write};
use std::process::{Child, Command, Stdio};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Mutex;

use serde_json::Value;

use crate::config::McpServerConfig;
use crate::error::McpError;
use crate::protocol::{
    initialize_request, initialized_notification, parse_call_tool_result, tools_call_request,
    JsonRpcNotification, JsonRpcRequest, JsonRpcResponse,
};
use crate::session::McpSession;

pub struct StdioMcpSession {
    child: Mutex<Child>,
    next_id: AtomicU64,
    initialized: Mutex<bool>,
}

impl StdioMcpSession {
    pub fn connect(config: &McpServerConfig) -> Result<Self, McpError> {
        let program = config
            .command
            .first()
            .ok_or_else(|| McpError::transport("stdio MCP server requires a command"))?;
        let mut command = Command::new(program);
        command
            .args(&config.command[1..])
            .envs(&config.env)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::null());
        let child = command.spawn().map_err(|err| {
            McpError::transport(format!("failed to spawn MCP server `{program}`: {err}"))
        })?;
        Ok(Self {
            child: Mutex::new(child),
            next_id: AtomicU64::new(1),
            initialized: Mutex::new(false),
        })
    }

    fn next_id(&self) -> u64 {
        self.next_id.fetch_add(1, Ordering::SeqCst)
    }

    fn write_request(&self, request: &JsonRpcRequest) -> Result<(), McpError> {
        let line = serde_json::to_string(request)
            .map_err(|err| McpError::transport(format!("failed to encode JSON-RPC request: {err}")))?;
        self.write_line(&line)
    }

    fn write_notification(&self, notification: &JsonRpcNotification) -> Result<(), McpError> {
        let line = serde_json::to_string(notification).map_err(|err| {
            McpError::transport(format!("failed to encode JSON-RPC notification: {err}"))
        })?;
        self.write_line(&line)
    }

    fn write_line(&self, line: &str) -> Result<(), McpError> {
        let mut child = self.child.lock().map_err(|_| {
            McpError::transport("MCP stdio session lock poisoned")
        })?;
        let stdin = child.stdin.as_mut().ok_or_else(|| {
            McpError::transport("MCP server stdin unavailable")
        })?;
        stdin
            .write_all(line.as_bytes())
            .and_then(|_| stdin.write_all(b"\n"))
            .map_err(|err| McpError::transport(format!("failed to write to MCP server stdin: {err}")))
    }

    fn read_response(&self, expected_id: u64) -> Result<JsonRpcResponse, McpError> {
        let mut child = self.child.lock().map_err(|_| {
            McpError::transport("MCP stdio session lock poisoned")
        })?;
        let stdout = child.stdout.as_mut().ok_or_else(|| {
            McpError::transport("MCP server stdout unavailable")
        })?;
        let mut reader = BufReader::new(stdout);
        loop {
            let mut line = String::new();
            reader
                .read_line(&mut line)
                .map_err(|err| McpError::transport(format!("failed to read MCP server stdout: {err}")))?;
            let trimmed = line.trim();
            if trimmed.is_empty() {
                continue;
            }
            let response: JsonRpcResponse = serde_json::from_str(trimmed).map_err(|err| {
                McpError::transport(format!("invalid JSON-RPC response from MCP server: {err}"))
            })?;
            if response.id.is_none() {
                continue;
            }
            let id_matches = response
                .id
                .as_ref()
                .and_then(|id| id.as_u64())
                .is_some_and(|id| id == expected_id);
            if id_matches {
                return Ok(response);
            }
        }
    }

    fn ensure_initialized(&self) -> Result<(), McpError> {
        let mut initialized = self.initialized.lock().map_err(|_| {
            McpError::transport("MCP stdio session lock poisoned")
        })?;
        if *initialized {
            return Ok(());
        }

        let init_id = self.next_id();
        self.write_request(&initialize_request(init_id))?;
        let init_response = self.read_response(init_id)?;
        if let Some(error) = init_response.error {
            return Err(McpError::transport(format!(
                "MCP initialize failed: {}",
                error.message
            )));
        }
        if init_response.result.is_none() {
            return Err(McpError::transport("MCP initialize returned no result"));
        }

        self.write_notification(&initialized_notification())?;
        *initialized = true;
        Ok(())
    }
}

impl McpSession for StdioMcpSession {
    fn call_tool(&self, tool: &str, arguments: Value) -> Result<(), McpError> {
        self.ensure_initialized()?;
        let request_id = self.next_id();
        self.write_request(&tools_call_request(request_id, tool, arguments))?;
        let response = self.read_response(request_id)?;
        if let Some(error) = response.error {
            return Err(McpError::tool_failed(
                tool,
                format!("JSON-RPC error {}: {}", error.code, error.message),
            ));
        }
        let result = response
            .result
            .ok_or_else(|| McpError::tool_failed(tool, "tools/call returned no result"))?;
        let call_result = parse_call_tool_result(result)
            .map_err(|message| McpError::tool_failed(tool, message))?;
        if call_result.is_error {
            let message = call_result
                .content
                .iter()
                .filter_map(|item| item.text.as_deref())
                .collect::<Vec<_>>()
                .join("\n");
            let message = if message.is_empty() {
                "tool returned isError=true".to_string()
            } else {
                message
            };
            return Err(McpError::tool_failed(tool, message));
        }
        Ok(())
    }
}

impl Drop for StdioMcpSession {
    fn drop(&mut self) {
        if let Ok(mut child) = self.child.lock() {
            let _ = child.kill();
            let _ = child.wait();
        }
    }
}
