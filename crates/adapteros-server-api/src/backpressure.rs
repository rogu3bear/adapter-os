use axum::{http::StatusCode, Json};

use adapteros_lora_worker::memory::MemoryPressureLevel;

use crate::state::AppState;
use crate::types::{ErrorResponse, UmaBackpressureError};

/// Enforce UMA backpressure guard before any worker selection.
pub fn check_uma_backpressure(state: &AppState) -> Result<(), (StatusCode, Json<ErrorResponse>)> {
    let pressure = state.uma_monitor.get_current_pressure();

    if matches!(
        pressure,
        MemoryPressureLevel::High | MemoryPressureLevel::Critical
    ) {
        let err = UmaBackpressureError::new(pressure.to_string());
        return Err((StatusCode::SERVICE_UNAVAILABLE, Json(err.into())));
    }

    Ok(())
}
