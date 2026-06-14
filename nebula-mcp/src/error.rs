use thiserror::Error;

#[derive(Debug, Error)]
pub enum McpError {
    #[error("NEB-P004 [probe_error] MCP transport error: {message}")]
    Transport { message: String },

    #[error("NEB-P003 [probe_error] MCP tool `{tool}` failed: {message}")]
    ToolFailed { tool: String, message: String },

    #[error("MCP configuration error: {message}")]
    Config { message: String },
}

impl McpError {
    pub fn transport(message: impl Into<String>) -> Self {
        Self::Transport {
            message: message.into(),
        }
    }

    pub fn tool_failed(tool: impl Into<String>, message: impl Into<String>) -> Self {
        Self::ToolFailed {
            tool: tool.into(),
            message: message.into(),
        }
    }

    pub fn config(message: impl Into<String>) -> Self {
        Self::Config {
            message: message.into(),
        }
    }
}
