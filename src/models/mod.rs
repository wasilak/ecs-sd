pub mod target;
pub use target::Target;

pub mod proxy_target;
pub use proxy_target::{build_routing_table, ProxyTarget};

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
    /// Metadata level override; falls back to configured default when omitted.
    pub level: Option<MetadataLevel>,
}
