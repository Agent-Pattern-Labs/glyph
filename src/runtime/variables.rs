use std::collections::BTreeMap;

use serde_json::Value;

#[derive(Debug, Default, Clone)]
pub struct VariableStore {
    values: BTreeMap<String, Value>,
}

impl VariableStore {
    pub fn set(&mut self, name: impl Into<String>, value: Value) {
        self.values.insert(name.into(), value);
    }

    pub fn get(&self, name: &str) -> Option<&Value> {
        self.values.get(name)
    }

    pub fn has(&self, name: &str) -> bool {
        self.values.contains_key(name)
    }

    pub fn snapshot(&self) -> BTreeMap<String, Value> {
        self.values.clone()
    }
}
