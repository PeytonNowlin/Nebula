use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

pub const PROTOCOL_VERSION: &str = "2024-11-05";

#[derive(Debug, Serialize)]
pub struct JsonRpcRequest {
    pub jsonrpc: &'static str,
    pub id: u64,
    pub method: &'static str,
    pub params: Value,
}

#[derive(Debug, Serialize)]
pub struct JsonRpcNotification {
    pub jsonrpc: &'static str,
    pub method: &'static str,
}

#[derive(Debug, Deserialize)]
pub struct JsonRpcResponse {
    pub jsonrpc: String,
    pub id: Option<Value>,
    pub result: Option<Value>,
    pub error: Option<JsonRpcErrorObject>,
}

#[derive(Debug, Deserialize)]
pub struct JsonRpcErrorObject {
    pub code: i64,
    pub message: String,
}

#[derive(Debug, Deserialize)]
pub struct CallToolResult {
    #[serde(default)]
    pub content: Vec<ToolContent>,
    #[serde(default)]
    pub is_error: bool,
}

#[derive(Debug, Deserialize)]
pub struct ToolContent {
    #[serde(rename = "type")]
    pub content_type: String,
    #[serde(default)]
    pub text: Option<String>,
}

pub fn initialize_request(id: u64) -> JsonRpcRequest {
    JsonRpcRequest {
        jsonrpc: "2.0",
        id,
        method: "initialize",
        params: json!({
            "protocolVersion": PROTOCOL_VERSION,
            "capabilities": {},
            "clientInfo": {
                "name": "nebula",
                "version": "0.1.0"
            }
        }),
    }
}

pub fn initialized_notification() -> JsonRpcNotification {
    JsonRpcNotification {
        jsonrpc: "2.0",
        method: "notifications/initialized",
    }
}

pub fn tools_call_request(id: u64, tool: &str, arguments: Value) -> JsonRpcRequest {
    JsonRpcRequest {
        jsonrpc: "2.0",
        id,
        method: "tools/call",
        params: json!({
            "name": tool,
            "arguments": arguments
        }),
    }
}

pub fn parse_call_tool_result(value: Value) -> Result<CallToolResult, String> {
    serde_json::from_value(value).map_err(|err| format!("invalid tools/call result: {err}"))
}
