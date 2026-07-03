pub mod target;
pub use target::Target;

pub mod proxy_target;
pub use proxy_target::{build_routing_table, ProxyTarget};

pub mod metadata_level;
pub use metadata_level::MetadataLevel;

pub mod label_builder;
pub use label_builder::LabelBuilder;

use serde::Deserialize;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum FilterMode {
    And,
    Or,
}

impl Default for FilterMode {
    fn default() -> Self {
        Self::And
    }
}

/// Query parameters for the /sd endpoint
#[derive(Debug, Deserialize)]
pub struct SdQueryParams {
    /// Filter by cluster name(s); repeatable (?cluster=a&cluster=b).
    #[serde(skip)]
    pub clusters: Vec<String>,
    /// Filter by ECS service name(s); repeatable.
    #[serde(skip)]
    pub services: Vec<String>,
    /// Filter by task definition family/families; repeatable.
    #[serde(skip)]
    pub families: Vec<String>,
    /// Metadata level override; falls back to configured default when omitted.
    pub level: Option<MetadataLevel>,
    #[serde(default)]
    pub filter_mode: FilterMode,
    #[serde(skip)]
    pub tag_filters: Vec<(String, String)>,
}

impl Default for SdQueryParams {
    fn default() -> Self {
        Self {
            clusters: Vec::new(),
            services: Vec::new(),
            families: Vec::new(),
            level: None,
            filter_mode: FilterMode::And,
            tag_filters: Vec::new(),
        }
    }
}
