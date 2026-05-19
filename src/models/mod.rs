pub mod target;
pub use target::Target;

use serde::Deserialize;

#[derive(Debug, Deserialize)]
pub struct FilterParams {
    pub cluster: Option<String>,
    pub service: Option<String>,
    pub family: Option<String>,
}
