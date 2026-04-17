pub mod face;
pub mod eye;
pub mod sharpness;
pub mod worker;
pub mod mock;

use serde::Serialize;

#[derive(Debug, Clone, Copy, Serialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum AiProviderStatus {
    Cuda,
    Cpu,
    Disabled,
}

#[derive(Debug, Clone)]
pub struct AiJob {
    pub shoot_id: i64,
    pub photo_id: i64,
    pub preview_path: String,
}
