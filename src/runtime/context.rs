use std::collections::BTreeMap;

use serde_json::{Map, Value};

use super::trace::ExecutionTrace;
use super::variables::VariableStore;

#[derive(Debug, Default)]
pub struct RuntimeContext {
    pub context: Map<String, Value>,
    pub variables: VariableStore,
    pub trace: ExecutionTrace,
    pub outputs: Vec<Value>,
    mock_fs: BTreeMap<String, Value>,
}

impl RuntimeContext {
    pub fn new(context: Map<String, Value>, mock_fs: BTreeMap<String, Value>) -> Self {
        Self {
            context,
            variables: VariableStore::default(),
            trace: ExecutionTrace::default(),
            outputs: Vec::new(),
            mock_fs,
        }
    }

    pub fn read_file(&self, path: &str) -> Option<&Value> {
        self.mock_fs.get(path)
    }

    pub fn write_file(&mut self, path: impl Into<String>, content: Value) {
        self.mock_fs.insert(path.into(), content);
    }

    pub fn snapshot_fs(&self) -> BTreeMap<String, Value> {
        self.mock_fs.clone()
    }
}
