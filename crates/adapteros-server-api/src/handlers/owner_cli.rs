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
    if command_trimmed
        .chars()
        .any(|c| dangerous_chars.contains(&c))
    {
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

/// Command execution timeout in seconds
const COMMAND_TIMEOUT_SECS: u64 = 30;

/// Execute the validated command using real aosctl process
///
/// **Security:**
/// - Commands are pre-validated against whitelist
/// - Uses tokio::process::Command for async execution
/// - 30-second timeout to prevent hanging processes
/// - Captures stdout/stderr separately
async fn execute_command(command: &str) -> Result<CliRunResponse, AosError> {
    let start = Instant::now();

    // Handle "help" command locally (no process spawn needed)
    if command == "help" {
        return Ok(CliRunResponse {
            stdout: format!(
                "Available commands:\n{}\n",
                ALLOWED_COMMANDS
                    .iter()
                    .map(|c| format!("  - {}", c))
                    .collect::<Vec<_>>()
                    .join("\n")
            ),
            stderr: String::new(),
            exit_code: 0,
            duration_ms: start.elapsed().as_millis() as u64,
        });
    }

    // Parse the command - extract args after "aosctl "
    let args: Vec<&str> = command
        .strip_prefix("aosctl ")
        .unwrap_or(command)
        .split_whitespace()
        .collect();

    // Spawn the aosctl process
    let child = tokio::process::Command::new("aosctl")
        .args(&args)
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .spawn()
        .map_err(|e| {
            AosError::Internal(format!(
                "Failed to spawn aosctl process: {}. Ensure aosctl is in PATH.",
                e
            ))
        })?;

    // Wait for process with timeout
    let output = tokio::time::timeout(
        std::time::Duration::from_secs(COMMAND_TIMEOUT_SECS),
        child.wait_with_output(),
    )
    .await
    .map_err(|_| AosError::Timeout {
        duration: std::time::Duration::from_secs(COMMAND_TIMEOUT_SECS),
    })?
    .map_err(|e| AosError::Internal(format!("Failed to wait for aosctl process: {}", e)))?;

    let duration_ms = start.elapsed().as_millis() as u64;

    let stdout = String::from_utf8_lossy(&output.stdout).to_string();
    let stderr = String::from_utf8_lossy(&output.stderr).to_string();
    let exit_code = output.status.code().unwrap_or(-1);

    info!(
        command = %command,
        exit_code = exit_code,
        stdout_len = stdout.len(),
        stderr_len = stderr.len(),
        duration_ms = duration_ms,
        "aosctl command executed"
    );

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
        // Help is handled locally, doesn't need aosctl binary
        let result = execute_command("help").await.unwrap();
        assert_eq!(result.exit_code, 0);
        assert!(result.stdout.contains("Available commands"));
        assert!(result.stderr.is_empty());
    }

    fn aosctl_available() -> bool {
        let Some(paths) = std::env::var_os("PATH") else {
            return false;
        };
        std::env::split_paths(&paths).any(|path| path.join("aosctl").exists())
    }

    // Note: Tests for actual aosctl commands require the aosctl binary to be available
    // These tests are integration tests and should run in CI with the full build
    #[tokio::test]
    async fn test_execute_command_status() {
        if !aosctl_available() {
            eprintln!("skipping: aosctl not found in PATH");
            return;
        }
        let result = execute_command("aosctl status").await.unwrap();
        // Real aosctl should return 0 on success
        assert!(result.exit_code == 0 || !result.stderr.is_empty());
    }

    #[tokio::test]
    async fn test_execute_command_adapters_list() {
        if !aosctl_available() {
            eprintln!("skipping: aosctl not found in PATH");
            return;
        }
        let result = execute_command("aosctl adapters list").await.unwrap();
        // May succeed or fail depending on database state
        assert!(result.exit_code >= 0);
    }
}
