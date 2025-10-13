//! API types (OpenAPI compatible)

use serde::{Deserialize, Serialize};

// Re-export worker types for API compatibility
pub use adapteros_lora_worker::{InferenceRequest, InferenceResponse};
