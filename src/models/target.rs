use serde::{Serialize, Deserialize};
use std::collections::HashMap;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Target {
    pub targets: Vec<String>,
    pub labels: HashMap<String, String>,
}

impl Target {
    /// Construct a Target from an IP, port, and label set.
    /// Produces a single-entry `targets` vector formatted as `"{ip}:{port}"`.
    pub fn new(ip: &str, port: u16, labels: HashMap<String, String>) -> Self {
        Self {
            targets: vec![format!("{}:{}", ip, port)],
            labels,
        }
    }
}
