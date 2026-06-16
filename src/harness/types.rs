use serde::{Deserialize, Serialize};
use serde_json::Value;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ToolStatus {
    Pass,
    Warning,
    Fail,
}

impl ToolStatus {
    pub fn as_str(self) -> &'static str {
        match self {
            ToolStatus::Pass => "pass",
            ToolStatus::Warning => "warning",
            ToolStatus::Fail => "fail",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ToolResult {
    pub status: ToolStatus,
    pub value: Value,
    pub summary: String,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub warnings: Vec<String>,
}
