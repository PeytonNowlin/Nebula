use serde_json::Value;

use crate::error::McpError;

pub trait McpSession: Send + Sync {
    fn call_tool(&self, tool: &str, arguments: Value) -> Result<(), McpError>;
}
