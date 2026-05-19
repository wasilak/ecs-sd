#[derive(Debug, Clone)]
pub struct Config {
    pub clusters: Vec<String>,
    pub listen: String,
    pub refresh_interval: u64,
    pub metadata_level: String,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            clusters: Vec::new(),
            listen: "0.0.0.0:8080".to_string(),
            refresh_interval: 60,
            metadata_level: "task".to_string(),
        }
    }
}

impl Config {
    pub fn new(clusters: Vec<String>) -> Self {
        Self {
            clusters,
            ..Default::default()
        }
    }
}
