//! BootStateManager trait implementation for adapteros-server-api's BootStateManager
//!
//! This module bridges the concrete BootStateManager from adapteros-server-api
//! to the BootStateManager trait required by admin handlers.

use crate::state::BootStateManager as BootStateManagerTrait;
use adapteros_boot::BootPhase as BootState;
use adapteros_server_api::boot_state::BootStateManager;

impl BootStateManagerTrait for BootStateManager {
    fn current_state(&self) -> BootState {
        self.current_state()
    }

    async fn drain(&self) {
        self.drain().await
    }

    async fn stop(&self) {
        self.stop().await
    }

    async fn maintenance(&self, reason: &str) {
        self.maintenance(reason).await
    }
}
