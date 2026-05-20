use serde::{Serialize, Deserialize};
use std::collections::HashMap;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Target {
    pub targets: Vec<String>,
    pub labels: HashMap<String, String>,
}

impl Target {
}
