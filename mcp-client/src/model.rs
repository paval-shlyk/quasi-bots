use rmcp::model::{CallToolResult, Content, RawContent, ServerInfo, Tool};
use serde_json::Value;

/// Snapshot of server identity after initialize.
#[derive(Debug, Clone)]
pub struct ServerStatus {
    pub name: String,
    pub version: String,
    pub protocol_version: String,
    pub instructions: Option<String>,
    pub tools_enabled: bool,
}

impl From<&ServerInfo> for ServerStatus {
    fn from(info: &ServerInfo) -> Self {
        Self {
            name: info.server_info.name.clone(),
            version: info.server_info.version.clone(),
            protocol_version: info.protocol_version.to_string(),
            instructions: info.instructions.clone(),
            tools_enabled: info.capabilities.tools.is_some(),
        }
    }
}

/// Tool metadata for the TUI list/detail panes.
#[derive(Debug, Clone)]
pub struct ToolView {
    pub name: String,
    pub description: String,
    pub input_schema: Value,
}

impl From<&Tool> for ToolView {
    fn from(tool: &Tool) -> Self {
        Self {
            name: tool.name.to_string(),
            description: tool
                .description
                .as_ref()
                .map(|d| d.to_string())
                .unwrap_or_default(),
            input_schema: Value::Object((*tool.input_schema).clone()),
        }
    }
}

/// Formatted tool-call outcome for display.
#[derive(Debug, Clone)]
pub struct CallOutcome {
    pub is_error: bool,
    pub text: String,
    pub structured: Option<Value>,
}

impl From<&CallToolResult> for CallOutcome {
    fn from(result: &CallToolResult) -> Self {
        let is_error = result.is_error.unwrap_or(false);
        let text = format_call_content(
            &result.content,
            result.structured_content.as_ref(),
        );
        Self {
            is_error,
            text,
            structured: result.structured_content.clone(),
        }
    }
}

fn format_call_content(
    content: &[Content],
    structured: Option<&Value>,
) -> String {
    if let Some(sc) = structured {
        return pretty_json(sc);
    }

    let mut parts = Vec::new();
    for block in content {
        match &block.raw {
            RawContent::Text(t) => {
                // Prefer pretty-printed JSON when the tool returned JSON text.
                if let Ok(v) = serde_json::from_str::<Value>(&t.text) {
                    parts.push(pretty_json(&v));
                } else {
                    parts.push(t.text.clone());
                }
            }
            RawContent::Image(_) => parts.push("[image content]".into()),
            RawContent::Audio(_) => parts.push("[audio content]".into()),
            RawContent::Resource(_) => parts.push("[embedded resource]".into()),
            RawContent::ResourceLink(r) => {
                parts.push(format!("[resource link: {}]", r.uri));
            }
        }
    }
    if parts.is_empty() {
        "(empty result)".into()
    } else {
        parts.join("\n\n")
    }
}

pub fn pretty_json(value: &Value) -> String {
    serde_json::to_string_pretty(value).unwrap_or_else(|_| value.to_string())
}

/// Log line shown in the TUI log pane.
#[derive(Debug, Clone)]
pub struct LogEntry {
    pub level: LogLevel,
    pub message: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LogLevel {
    Info,
    Warn,
    Error,
    Success,
}

impl LogEntry {
    pub fn info(msg: impl Into<String>) -> Self {
        Self {
            level: LogLevel::Info,
            message: msg.into(),
        }
    }

    pub fn warn(msg: impl Into<String>) -> Self {
        Self {
            level: LogLevel::Warn,
            message: msg.into(),
        }
    }

    pub fn error(msg: impl Into<String>) -> Self {
        Self {
            level: LogLevel::Error,
            message: msg.into(),
        }
    }

    pub fn success(msg: impl Into<String>) -> Self {
        Self {
            level: LogLevel::Success,
            message: msg.into(),
        }
    }
}
