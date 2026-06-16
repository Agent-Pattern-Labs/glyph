use std::collections::BTreeMap;
use std::sync::Arc;

use serde_json::{Map, Value};

use crate::harness::types::ToolResult;
use crate::runtime::context::RuntimeContext;
use crate::runtime::errors::GlyphRuntimeError;

pub type ToolHandler = Arc<
    dyn Fn(Map<String, Value>, &mut RuntimeContext) -> Result<ToolResult, GlyphRuntimeError>
        + Send
        + Sync,
>;

#[derive(Clone, Default)]
pub struct ToolRegistry {
    tools: BTreeMap<String, ToolHandler>,
}

impl ToolRegistry {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn register<F>(&mut self, name: &str, handler: F) -> &mut Self
    where
        F: Fn(Map<String, Value>, &mut RuntimeContext) -> Result<ToolResult, GlyphRuntimeError>
            + Send
            + Sync
            + 'static,
    {
        self.tools.insert(name.to_uppercase(), Arc::new(handler));
        self
    }

    pub fn get(&self, name: &str) -> Option<ToolHandler> {
        self.tools.get(&name.to_uppercase()).cloned()
    }

    pub fn names(&self) -> Vec<String> {
        self.tools.keys().cloned().collect()
    }
}
