//! Owner CLI Runner Endpoint
//!
//! Provides a secure API endpoint for executing whitelisted aosctl commands
//! from the Admin-role UI. Commands are validated, executed in a controlled
//! environment, and audited.
//!
//! **Security Features:**
//! - Command whitelist validation
//! - Injection attack prevention (pipes, redirects, shell metacharacters)
//! - Audit logging for all executions
//! - Admin role requirement
//!
//! **Supported Commands:**
//! - `aosctl status`
//! - `aosctl adapters list`
//! - `aosctl adapters describe <id>`
//! - `aosctl models list`
//! - `aosctl models status`
//! - `aosctl tenant list`
//! - `aosctl stack list`
//! - `aosctl stack describe <id>`
//! - `aosctl logs`
//! - `help`

use crate::audit_helper::log_action;
use crate::auth::Claims;
use crate::state::AppState;
use adapteros_api_types::ErrorResponse;
use adapteros_core::AosError;
use axum::{extract::State, http::StatusCode, response::Json, Extension};
use serde::{Deserialize, Serialize};
use std::time::Instant;
use tracing::{error, info, warn};
use utoipa::ToSchema;

/// CLI command execution request
#[derive(Debug, Deserialize, ToSchema)]
pub struct CliRunRequest {
    /// Command string to execute (e.g., "aosctl status")
    pub command: String,
    /// Optional session ID for tracking related commands
    pub session_id: Option<String>,
}

/// CLI command execution response
#[derive(Debug, Serialize, ToSchema)]
pub struct CliRunResponse {
    /// Standard output from the command
    pub stdout: String,
    /// Standard error from the command
    pub stderr: String,
    /// Exit code (0 = success)
    pub exit_code: i32,
    /// Execution duration in milliseconds
    pub duration_ms: u64,
}

/// Whitelist of allowed commands
const ALLOWED_COMMANDS: &[&str] = &[
    "aosctl status",
    "aosctl adapters list",
    "aosctl adapters describe",
    "aosctl models list",
    "aosctl models status",
    "aosctl tenant list",
    "aosctl stack list",
    "aosctl stack describe",
    "aosctl logs",
    "help",
];

/// Execute a whitelisted CLI command
///
/// **Permissions:** Requires `admin` role (highest privilege level).
///
/// **Security:**
/// - Commands are validated against a strict whitelist
/// - Shell metacharacters are rejected (pipes, redirects, semicolons, etc.)
/// - All executions are audit logged
///
/// **Audit Events:**
/// - Success: `cli.owner_run` with command details
/// - Failure: `cli.owner_run` with error message
///
/// # Example
/// ```json
/// POST /v1/cli/owner-run
/// {
///   "command": "aosctl status",
///   "session_id": "session-123"
/// }
/// ```
///
/// **Response:**
/// ```json
/// {
///   "stdout": "Status: Running\nVersion: 0.3-alpha\n",
///   "stderr": "",
///   "exit_code": 0,
///   "duration_ms": 42
/// }
/// ```
#[utoipa::path(
    post,
    path = "/v1/cli/owner-run",
    request_body = CliRunRequest,
    responses(
        (status = 200, description = "Command executed successfully", body = CliRunResponse),
        (status = 400, description = "Invalid command", body = ErrorResponse),
        (status = 403, description = "Forbidden - Admin role required", body = ErrorResponse),
        (status = 500, description = "Command execution failed", body = ErrorResponse)
    ),
    tag = "cli"
)]
pub async fn run_owner_cli_command(
    State(state): State<AppState>,
    Extension(claims): Extension<Claims>,
    Json(req): Json<CliRunRequest>,
) -> Result<Json<CliRunResponse>, (StatusCode, Json<ErrorResponse>)> {
    let start = Instant::now();

    // Validate Admin role
    if claims.role != "admin" && !claims.roles.contains(&"admin".to_string()) {
        warn!(
            user_id = %claims.sub,
            role = %claims.role,
            command = %req.command,
            "Admin CLI access denied: insufficient permissions"
        );

        log_action(
            &state.db,
            &claims,
            "cli.owner_run",
            "cli_command",
            Some(&req.command),
            "failure",
            Some("Admin role required"),
        )
        .await
        .ok();

        return Err((
            StatusCode::FORBIDDEN,
            Json(
                ErrorResponse::new("Admin role required to execute CLI commands")
                    .with_code("FORBIDDEN"),
            ),
        ));
    }

    // Validate command against whitelist and security rules
    if let Err(e) = validate_command(&req.command) {
        warn!(
            user_id = %claims.sub,
            command = %req.command,
            error = %e,
            "Owner CLI command rejected: validation failed"
        );

        log_action(
            &state.db,
            &claims,
            "cli.owner_run",
            "cli_command",
            Some(&req.command),
            "failure",
            Some(&e.to_string()),
        )
        .await
        .ok();

        return Err((
            StatusCode::BAD_REQUEST,
            Json(ErrorResponse::new(e.to_string()).with_code("INVALID_COMMAND")),
        ));
    }

    info!(
        user_id = %claims.sub,
        tenant_id = %claims.tenant_id,
        command = %req.command,
        session_id = ?req.session_id,
        "Executing Owner CLI command"
    );

    // Execute command (mock for now)
    let result = execute_command(&req.command).await;

    let duration_ms = start.elapsed().as_millis() as u64;

    match result {
        Ok(response) => {
            info!(
                user_id = %claims.sub,
                command = %req.command,
                exit_code = response.exit_code,
                duration_ms = duration_ms,
                "Owner CLI command completed successfully"
            );

            log_action(
                &state.db,
                &claims,
                "cli.owner_run",
                "cli_command",
                Some(&req.command),
                "success",
                None,
            )
            .await
            .ok();

            Ok(Json(response))
        }
        Err(e) => {
            error!(
                user_id = %claims.sub,
                command = %req.command,
                error = %e,
                duration_ms = duration_ms,
                "Owner CLI command execution failed"
            );

            log_action(
                &state.db,
                &claims,
                "cli.owner_run",
                "cli_command",
                Some(&req.command),
                "failure",
                Some(&e.to_string()),
            )
            .await
            .ok();

            Err((
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(
                    ErrorResponse::new(format!("Command execution failed: {}", e))
                        .with_code("EXECUTION_FAILED"),
                ),
            ))
        }
    }
}

/// Validate command against whitelist and security rules
fn validate_command(command: &str) -> Result<(), AosError> {
    let command_trimmed = command.trim();

    // Reject empty commands
    if command_trimmed.is_empty() {
        return Err(AosError::Validation("Command cannot be empty".to_string()));
    }

    // Check for shell metacharacters (injection prevention)
    let dangerous_chars = ['|', '>', '<', ';', '&', '`', '$', '(', ')'];
    if command_trimmed.chars().any(|c| dangerous_chars.contains(&c)) {
        return Err(AosError::Validation(
            "Command contains forbidden characters (pipes, redirects, or shell metacharacters)"
                .to_string(),
        ));
    }

    // Special case: "help" command
    if command_trimmed == "help" {
        return Ok(());
    }

    // Ensure command starts with "aosctl"
    if !command_trimmed.starts_with("aosctl ") {
        return Err(AosError::Validation(
            "Command must start with 'aosctl' or be 'help'".to_string(),
        ));
    }

    // Check against whitelist
    let is_whitelisted = ALLOWED_COMMANDS.iter().any(|allowed| {
        // Allow exact match or match with additional arguments
        // e.g., "aosctl adapters describe" allows "aosctl adapters describe my-adapter"
        command_trimmed == *allowed || command_trimmed.starts_with(&format!("{} ", allowed))
    });

    if !is_whitelisted {
        return Err(AosError::Validation(format!(
            "Command '{}' is not in the allowed whitelist. Allowed commands: {}",
            command_trimmed,
            ALLOWED_COMMANDS.join(", ")
        )));
    }

    Ok(())
}

/// Execute the validated command (mock implementation)
///
/// **TODO:** Replace with actual aosctl execution
/// - Use tokio::process::Command to spawn aosctl
/// - Capture stdout/stderr
/// - Set appropriate timeout (e.g., 30 seconds)
/// - Handle process failures gracefully
async fn execute_command(command: &str) -> Result<CliRunResponse, AosError> {
    let start = Instant::now();

    // Mock implementation - simulate command execution
    let (stdout, stderr, exit_code) = match command {
        "help" => (
            "Available commands:\n\
             - aosctl status\n\
             - aosctl adapters list\n\
             - aosctl models list\n\
             - aosctl tenant list\n\
             - aosctl stack list\n"
                .to_string(),
            String::new(),
            0,
        ),
        cmd if cmd.starts_with("aosctl status") => (
            "AdapterOS Status\n\
             Version: 0.3-alpha\n\
             Status: Running\n\
             Workers: 2 active\n\
             Adapters: 5 loaded\n"
                .to_string(),
            String::new(),
            0,
        ),
        cmd if cmd.starts_with("aosctl adapters list") => (
            "Adapter ID               Tier      Rank  State\n\
             rust-expert             tier_1    16    warm\n\
             python-assistant        tier_1    12    cold\n\
             code-review             tier_2    8     hot\n"
                .to_string(),
            String::new(),
            0,
        ),
        cmd if cmd.starts_with("aosctl models list") => (
            "Model ID                Backend   State\n\
             qwen2.5-7b-mlx          mlx       loaded\n"
                .to_string(),
            String::new(),
            0,
        ),
        cmd if cmd.starts_with("aosctl tenant list") => (
            "Tenant ID     Status    Adapters\n\
             default       active    3\n\
             tenant-a      active    2\n"
                .to_string(),
            String::new(),
            0,
        ),
        cmd if cmd.starts_with("aosctl stack list") => (
            "Stack ID              Adapters  Workflow\n\
             default-stack         2         inference\n\
             code-review-stack     3         code-review\n"
                .to_string(),
            String::new(),
            0,
        ),
        cmd if cmd.starts_with("aosctl logs") => (
            "[2025-11-25 14:32:01] INFO: System started\n\
             [2025-11-25 14:32:15] INFO: Adapter loaded: rust-expert\n\
             [2025-11-25 14:33:42] INFO: Inference completed: 127ms\n"
                .to_string(),
            String::new(),
            0,
        ),
        _ => (
            String::new(),
            format!("Error: Command not implemented in mock: {}", command),
            1,
        ),
    };

    let duration_ms = start.elapsed().as_millis() as u64;

    Ok(CliRunResponse {
        stdout,
        stderr,
        exit_code,
        duration_ms,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_validate_command_whitelist() {
        // Valid commands
        assert!(validate_command("aosctl status").is_ok());
        assert!(validate_command("aosctl adapters list").is_ok());
        assert!(validate_command("aosctl adapters describe my-adapter").is_ok());
        assert!(validate_command("help").is_ok());

        // Invalid commands
        assert!(validate_command("aosctl delete").is_err());
        assert!(validate_command("rm -rf /").is_err());
        assert!(validate_command("").is_err());
    }

    #[test]
    fn test_validate_command_injection_prevention() {
        // Shell metacharacters should be rejected
        assert!(validate_command("aosctl status | grep running").is_err());
        assert!(validate_command("aosctl status > output.txt").is_err());
        assert!(validate_command("aosctl status; rm -rf /").is_err());
        assert!(validate_command("aosctl status && malicious").is_err());
        assert!(validate_command("aosctl status `whoami`").is_err());
        assert!(validate_command("aosctl status $(whoami)").is_err());
    }

    #[test]
    fn test_validate_command_prefix() {
        // Commands must start with "aosctl" or be "help"
        assert!(validate_command("status").is_err());
        assert!(validate_command("adapters list").is_err());
        assert!(validate_command("help").is_ok());
        assert!(validate_command("aosctl status").is_ok());
    }

    #[tokio::test]
    async fn test_execute_command_help() {
        let result = execute_command("help").await.unwrap();
        assert_eq!(result.exit_code, 0);
        assert!(result.stdout.contains("Available commands"));
        assert!(result.stderr.is_empty());
    }

    #[tokio::test]
    async fn test_execute_command_status() {
        let result = execute_command("aosctl status").await.unwrap();
        assert_eq!(result.exit_code, 0);
        assert!(result.stdout.contains("Version"));
        assert!(result.stderr.is_empty());
    }

    #[tokio::test]
    async fn test_execute_command_unknown() {
        let result = execute_command("aosctl unknown").await.unwrap();
        assert_eq!(result.exit_code, 1);
        assert!(!result.stderr.is_empty());
    }
}
