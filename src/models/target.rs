use serde::{Serialize, Deserialize};
use std::collections::HashMap;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Target {
    pub targets: Vec<String>,
    pub labels: HashMap<String, String>,
}

impl Target {
    pub fn new(address: String) -> Self {
        Self {
            targets: vec![address],
            labels: HashMap::new(),
        }
    }

    pub fn with_label(mut self, key: impl Into<String>, value: impl Into<String>) -> Self {
        self.labels.insert(key.into(), value.into());
        self
    }
}
