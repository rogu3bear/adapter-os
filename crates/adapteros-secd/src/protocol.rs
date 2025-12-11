//! UDS Protocol for Secure Enclave Daemon
//!
//! Simple JSON-based request/response protocol over Unix Domain Sockets.

use base64::Engine;
use serde::{Deserialize, Serialize};

/// Request to enclave daemon
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "op", rename_all = "snake_case")]
pub enum Request {
    /// Sign data with enclave-backed key
    Sign {
        /// Base64-encoded data to sign
        data: String,
        /// Optional key label (defaults to "aos_bundle_signing")
        #[serde(default)]
        key_label: Option<String>,
    },

    /// Encrypt data for at-rest storage
    Seal {
        /// Base64-encoded data to encrypt
        data: String,
    },

    /// Decrypt previously sealed data
    Unseal {
        /// Base64-encoded encrypted data
        data: String,
    },

    /// Ensure a tenant-specific key exists
    EnsureTenantKey {
        /// Tenant identifier used for key derivation
        tenant_id: String,
    },

    /// Encrypt data with a tenant-specific key
    SealTenant {
        /// Tenant identifier
        tenant_id: String,
        /// Base64-encoded data to encrypt
        data: String,
    },

    /// Decrypt data with a tenant-specific key
    UnsealTenant {
        /// Tenant identifier
        tenant_id: String,
        /// Base64-encoded encrypted data
        data: String,
    },

    /// Compute keyed digest with tenant-specific key
    DigestTenant {
        /// Tenant identifier
        tenant_id: String,
        /// Base64-encoded data to hash
        data: String,
    },

    /// Export tenant key material (protected by permission token)
    ExportTenantKey {
        /// Tenant identifier
        tenant_id: String,
        /// Permission token required for export
        permission_token: String,
    },

    /// Get public key for verification
    GetPublicKey {
        /// Key label
        key_label: String,
    },

    /// Health check
    Ping,
}

/// Response from enclave daemon
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "status", rename_all = "snake_case")]
pub enum Response {
    /// Operation succeeded
    Ok {
        /// Base64-encoded result data (signature, encrypted data, etc.)
        #[serde(skip_serializing_if = "Option::is_none")]
        result: Option<String>,
    },

    /// Operation failed
    Error {
        /// Error message
        message: String,
    },
}

impl Response {
    /// Create success response with data
    pub fn ok(data: Vec<u8>) -> Self {
        Response::Ok {
            result: Some(base64::engine::general_purpose::STANDARD.encode(data)),
        }
    }

    /// Create success response with no data
    pub fn ok_empty() -> Self {
        Response::Ok { result: None }
    }

    /// Create error response
    pub fn error(message: impl Into<String>) -> Self {
        Response::Error {
            message: message.into(),
        }
    }
}
