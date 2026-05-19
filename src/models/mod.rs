pub mod target;
pub use target::Target;

pub mod metadata_level;
pub use metadata_level::MetadataLevel;

pub mod label_builder;
pub use label_builder::LabelBuilder;

use serde::Deserialize;

#[derive(Debug, Deserialize)]
pub struct FilterParams {
    pub cluster: Option<String>,
    pub service: Option<String>,
    pub family: Option<String>,
}
