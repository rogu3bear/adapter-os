//! UdsClient trait implementation for adapteros-server-api's UdsClient
//!
//! This module bridges the concrete UdsClient from adapteros-server-api
//! to the UdsClient trait required by admin handlers.

use crate::state::{MaintenanceSignalResponse, UdsClient as UdsClientTrait};
use adapteros_core::Result;
use adapteros_server_api::uds_client::UdsClient;
use std::path::Path;

impl UdsClientTrait for UdsClient {
    async fn signal_maintenance(
        &self,
        path: &Path,
        mode: &str,
        reason: Option<&str>,
    ) -> Result<MaintenanceSignalResponse> {
        let response = self
            .signal_maintenance(path, mode, reason)
            .await
            .map_err(|e| adapteros_core::AosError::Network(e.to_string()))?;

        Ok(MaintenanceSignalResponse {
            mode: response.mode,
            drain_flag_set: response.drain_flag_set,
        })
    }
}
