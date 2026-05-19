pub mod target;
pub use target::Target;

pub mod metadata_level;
pub use metadata_level::MetadataLevel;

pub mod label_builder;
pub use label_builder::LabelBuilder;

use serde::Deserialize;

/// Query parameters for the /sd endpoint
#[derive(Debug, Deserialize)]
pub struct SdQueryParams {
    pub cluster: Option<String>,
    pub service: Option<String>,
    pub family: Option<String>,
    /// Metadata level to return (default: from config, typically "task")
    #[serde(default)]
    pub level: MetadataLevel,
}

/// Legacy filter params - kept for compatibility
#[derive(Debug, Deserialize)]
pub struct FilterParams {
    pub cluster: Option<String>,
    pub service: Option<String>,
    pub family: Option<String>,
}
