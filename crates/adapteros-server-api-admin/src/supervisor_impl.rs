//! SupervisorClient trait implementation for adapteros-server-api's SupervisorClient
//!
//! This module bridges the concrete SupervisorClient from adapteros-server-api
//! to the SupervisorClient trait required by admin handlers.

use crate::state::{SupervisorClient as SupervisorClientTrait, SupervisorError};
use adapteros_core::AosError;
use adapteros_server_api::supervisor_client::SupervisorClient;

impl SupervisorClientTrait for SupervisorClient {
    async fn start_service(
        &self,
        service_id: &str,
    ) -> std::result::Result<String, SupervisorError> {
        self.start_service(service_id)
            .await
            .map_err(|e| map_aos_error_to_supervisor_error(e))
    }

    async fn stop_service(
        &self,
        service_id: &str,
    ) -> std::result::Result<String, SupervisorError> {
        self.stop_service(service_id)
            .await
            .map_err(|e| map_aos_error_to_supervisor_error(e))
    }

    async fn restart_service(
        &self,
        service_id: &str,
    ) -> std::result::Result<String, SupervisorError> {
        self.restart_service(service_id)
            .await
            .map_err(|e| map_aos_error_to_supervisor_error(e))
    }

    async fn start_essential_services(&self) -> std::result::Result<String, SupervisorError> {
        self.start_essential_services()
            .await
            .map_err(|e| map_aos_error_to_supervisor_error(e))
    }

    async fn stop_essential_services(&self) -> std::result::Result<String, SupervisorError> {
        self.stop_essential_services()
            .await
            .map_err(|e| map_aos_error_to_supervisor_error(e))
    }

    async fn get_service_logs(
        &self,
        service_id: &str,
        lines: Option<u32>,
    ) -> std::result::Result<Vec<String>, SupervisorError> {
        self.get_service_logs(service_id, lines)
            .await
            .map_err(|e| map_aos_error_to_supervisor_error(e))
    }
}

/// Map AosError to SupervisorError, detecting not-found errors
fn map_aos_error_to_supervisor_error(e: AosError) -> SupervisorError {
    match e {
        AosError::NotFound(msg) => SupervisorError::not_found(msg),
        other => SupervisorError::new(other.to_string()),
    }
}
