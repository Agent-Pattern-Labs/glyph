use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};

use crate::harness::types::ToolStatus;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TraceEvent {
    #[serde(rename = "stepId")]
    pub step_id: String,
    pub operation: String,
    #[serde(rename = "resolvedArgs")]
    pub resolved_args: Map<String, Value>,
    #[serde(rename = "outputSummary")]
    pub output_summary: String,
    pub status: ToolStatus,
    #[serde(rename = "durationMs")]
    pub duration_ms: u128,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub errors: Vec<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub iteration: Option<usize>,
}

#[derive(Debug, Default, Clone)]
pub struct ExecutionTrace {
    events: Vec<TraceEvent>,
}

impl ExecutionTrace {
    pub fn add(&mut self, event: TraceEvent) {
        self.events.push(event);
    }

    pub fn all(&self) -> Vec<TraceEvent> {
        self.events.clone()
    }
}
