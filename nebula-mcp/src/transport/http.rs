use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Mutex;

use serde_json::Value;
use ureq::Agent;

use crate::config::McpServerConfig;
use crate::error::McpError;
use crate::protocol::{
    initialize_request, initialized_notification, parse_call_tool_result, parse_tools_list_result,
    tools_call_request, tools_list_request, JsonRpcNotification, JsonRpcRequest, JsonRpcResponse,
    McpToolDescriptor,
};
use crate::session::McpSession;

pub struct HttpMcpSession {
    agent: Agent,
    url: String,
    headers: Vec<(String, String)>,
    session_id: Mutex<Option<String>>,
    next_id: AtomicU64,
    initialized: Mutex<bool>,
}

impl HttpMcpSession {
    pub fn connect(config: &McpServerConfig) -> Result<Self, McpError> {
        let url = config.url.clone().ok_or_else(|| {
            McpError::transport("http MCP server requires a url")
        })?;
        let headers = config
            .headers
            .iter()
            .map(|(k, v)| (k.clone(), v.clone()))
            .collect();
        Ok(Self {
            agent: Agent::new_with_defaults(),
            url,
            headers,
            session_id: Mutex::new(None),
            next_id: AtomicU64::new(1),
            initialized: Mutex::new(false),
        })
    }

    fn next_id(&self) -> u64 {
        self.next_id.fetch_add(1, Ordering::SeqCst)
    }

    fn post_json(&self, body: &str) -> Result<(String, Option<String>), McpError> {
        let mut request = self
            .agent
            .post(&self.url)
            .header("Content-Type", "application/json")
            .header("Accept", "application/json, text/event-stream");
        for (key, value) in &self.headers {
            request = request.header(key, value);
        }
        if let Ok(guard) = self.session_id.lock() {
            if let Some(session_id) = guard.as_ref() {
                request = request.header("Mcp-Session-Id", session_id);
            }
        }

        let response = request.send(body).map_err(|err| {
            McpError::transport(format!("HTTP request to MCP server failed: {err}"))
        })?;
        let status = response.status().as_u16();
        let session_id = response
            .headers()
            .get("Mcp-Session-Id")
            .map(|value| value.to_str().unwrap_or_default().to_string())
            .filter(|value| !value.is_empty());
        let text = response.into_body().read_to_string().map_err(|err| {
            McpError::transport(format!("failed to read MCP HTTP response body: {err}"))
        })?;
        if !(200..300).contains(&status) {
            return Err(McpError::transport(format!(
                "MCP HTTP server returned status {status}: {text}"
            )));
        }
        Ok((text, session_id))
    }

    fn send_request(&self, request: &JsonRpcRequest) -> Result<JsonRpcResponse, McpError> {
        let body = serde_json::to_string(request)
            .map_err(|err| McpError::transport(format!("failed to encode JSON-RPC request: {err}")))?;
        let (text, session_id) = self.post_json(&body)?;
        if let Some(session_id) = session_id {
            if let Ok(mut guard) = self.session_id.lock() {
                *guard = Some(session_id);
            }
        }
        Self::parse_response_body(&text, request.id)
    }

    fn send_notification(&self, notification: &JsonRpcNotification) -> Result<(), McpError> {
        let body = serde_json::to_string(notification).map_err(|err| {
            McpError::transport(format!("failed to encode JSON-RPC notification: {err}"))
        })?;
        let (_text, session_id) = self.post_json(&body)?;
        if let Some(session_id) = session_id {
            if let Ok(mut guard) = self.session_id.lock() {
                *guard = Some(session_id);
            }
        }
        Ok(())
    }

    fn parse_response_body(body: &str, expected_id: u64) -> Result<JsonRpcResponse, McpError> {
        let trimmed = body.trim();
        if trimmed.starts_with("event:") || trimmed.contains("\ndata:") {
            return Self::parse_sse_body(trimmed, expected_id);
        }
        let response: JsonRpcResponse = serde_json::from_str(trimmed).map_err(|err| {
            McpError::transport(format!("invalid JSON-RPC HTTP response: {err}"))
        })?;
        Self::validate_response_id(&response, expected_id)?;
        Ok(response)
    }

    fn parse_sse_body(body: &str, expected_id: u64) -> Result<JsonRpcResponse, McpError> {
        for line in body.lines() {
            let line = line.trim();
            if let Some(data) = line.strip_prefix("data:") {
                let data = data.trim();
                if data.is_empty() {
                    continue;
                }
                if let Ok(response) = serde_json::from_str::<JsonRpcResponse>(data) {
                    if response.id.is_some() {
                        Self::validate_response_id(&response, expected_id)?;
                        return Ok(response);
                    }
                }
            }
        }
        Err(McpError::transport(
            "MCP HTTP SSE response did not contain a matching JSON-RPC response",
        ))
    }

    fn validate_response_id(response: &JsonRpcResponse, expected_id: u64) -> Result<(), McpError> {
        let id = response
            .id
            .as_ref()
            .and_then(|id| id.as_u64())
            .ok_or_else(|| McpError::transport("MCP HTTP response missing id"))?;
        if id != expected_id {
            return Err(McpError::transport(format!(
                "MCP HTTP response id mismatch: expected {expected_id}, got {id}"
            )));
        }
        Ok(())
    }

    fn ensure_initialized(&self) -> Result<(), McpError> {
        let mut initialized = self.initialized.lock().map_err(|_| {
            McpError::transport("MCP HTTP session lock poisoned")
        })?;
        if *initialized {
            return Ok(());
        }

        let init_id = self.next_id();
        let init_response = self.send_request(&initialize_request(init_id))?;
        if let Some(error) = init_response.error {
            return Err(McpError::transport(format!(
                "MCP initialize failed: {}",
                error.message
            )));
        }
        if init_response.result.is_none() {
            return Err(McpError::transport("MCP initialize returned no result"));
        }

        self.send_notification(&initialized_notification())?;
        *initialized = true;
        Ok(())
    }
}

impl McpSession for HttpMcpSession {
    fn call_tool(&self, tool: &str, arguments: Value) -> Result<(), McpError> {
        self.ensure_initialized()?;
        let request_id = self.next_id();
        let response = self.send_request(&tools_call_request(request_id, tool, arguments))?;
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

    fn list_tools(&self) -> Result<Vec<McpToolDescriptor>, McpError> {
        self.ensure_initialized()?;
        let request_id = self.next_id();
        let response = self.send_request(&tools_list_request(request_id))?;
        if let Some(error) = response.error {
            return Err(McpError::transport(format!(
                "tools/list failed: {}",
                error.message
            )));
        }
        let result = response
            .result
            .ok_or_else(|| McpError::transport("tools/list returned no result"))?;
        parse_tools_list_result(result)
            .map(|list| list.tools)
            .map_err(McpError::transport)
    }
}
