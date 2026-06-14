use serde_json::Value;

use crate::error::McpError;
use crate::protocol::McpToolDescriptor;

pub trait McpSession: Send + Sync {
    fn call_tool(&self, tool: &str, arguments: Value) -> Result<(), McpError>;
    fn list_tools(&self) -> Result<Vec<McpToolDescriptor>, McpError>;
}
