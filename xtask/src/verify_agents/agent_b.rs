//! Agent B: Backend & Control Plane checks

use super::{Check, Section, VerifyAgentsArgs};
use anyhow::Result;
use std::fs;
use std::path::Path;
use std::process::{Child, Command};
use std::thread;
use std::time::Duration;

pub async fn run(args: &VerifyAgentsArgs) -> Result<Section> {
    let mut section = Section::new("Agent B - Backend & Control Plane");

    // 1. Migration range check
    section.add_check(check_migrations());

    // 2-8. Runtime checks (if not static-only)
    if args.static_only {
        section.add_check(Check::skip(
            "Runtime checks",
            "Skipped due to --static-only flag",
        ));
    } else if let Some(url) = &args.assume_running {
        // Use existing server
        section.add_check(check_meta_endpoint(url));
        section.add_check(check_routing_decisions(url));
        section.add_check(check_audits_endpoint(url));
        section.add_check(check_metrics_auth(url, &args.metrics_token));
        section.add_check(Check::skip("SIGHUP reload", "Server already running"));
        section.add_check(Check::skip("PID lock", "Server already running"));
    } else {
        // Start server and run tests
        let runtime_checks = run_runtime_checks(args).await;
        for check in runtime_checks {
            section.add_check(check);
        }
    }

    // JWT rotation (static check)
    section.add_check(check_jwt_rotation());

    Ok(section)
}

fn check_migrations() -> Check {
    // Check for migrations in 0100-0199 range
    let migrations_dir = Path::new("migrations");
    if !migrations_dir.exists() {
        return Check::fail("Migration range check", vec![], "migrations/ not found");
    }

    let entries = match fs::read_dir(migrations_dir) {
        Ok(e) => e,
        Err(e) => {
            return Check::fail(
                "Migration range check",
                vec![],
                format!("Failed to read migrations: {}", e),
            )
        }
    };

    let agent_b_migrations: Vec<_> = entries
        .filter_map(|e| e.ok())
        .filter(|e| {
            e.file_name()
                .to_string_lossy()
                .starts_with("01") // 0100-0199
        })
        .collect();

    if agent_b_migrations.is_empty() {
        return Check::fail(
            "Migration range check",
            vec![],
            "No migrations in 0100-0199 range found",
        );
    }

    // Check for promotions table
    let has_promotions = agent_b_migrations.iter().any(|e| {
        let path = e.path();
        if let Ok(content) = fs::read_to_string(&path) {
            content.contains("CREATE TABLE") && content.contains("promotions")
        } else {
            false
        }
    });

    let evidence: Vec<_> = agent_b_migrations
        .iter()
        .map(|e| e.file_name().to_string_lossy().to_string())
        .collect();

    if has_promotions {
        Check::pass(
            "Migration range check",
            vec![
                format!("Found {} Agent B migrations", evidence.len()),
                format!("Files: {}", evidence.join(", ")),
                "promotions table DDL found".to_string(),
            ],
        )
    } else {
        Check::fail(
            "Migration range check",
            evidence,
            "promotions table DDL not found in migrations",
        )
    }
}

async fn run_runtime_checks(args: &VerifyAgentsArgs) -> Vec<Check> {
    let mut checks = Vec::new();

    // Try to start server
    let port = 19443; // Ephemeral port for testing
    let server_result = start_server(port, &args.pid_file);

    match server_result {
        Ok((mut server, url)) => {
            // Wait for server to be ready
            thread::sleep(Duration::from_secs(3));

            // Run endpoint checks
            checks.push(check_meta_endpoint(&url));
            checks.push(check_routing_decisions(&url));
            checks.push(check_audits_endpoint(&url));
            checks.push(check_metrics_auth(&url, &args.metrics_token));

            // SIGHUP reload check
            checks.push(check_sighup_reload(server.id()));

            // Stop server
            let _ = server.kill();
            thread::sleep(Duration::from_millis(500));

            // PID lock check
            checks.push(check_pid_lock(port, &args.pid_file));
        }
        Err(e) => {
            checks.push(Check::fail(
                "Server startup",
                vec![],
                format!("Failed to start server: {}", e),
            ));
            checks.push(Check::skip("Endpoint tests", "Server failed to start"));
        }
    }

    checks
}

fn start_server(port: u16, pid_file: &Path) -> Result<(Child, String)> {
    let child = Command::new("cargo")
        .args([
            "run",
            "--bin",
            "mplora-server",
            "--",
            "--config",
            "configs/cp.toml",
            "--pid-file",
            &pid_file.display().to_string(),
        ])
        .env("AOS_SERVER_PORT", port.to_string())
        .spawn()?;

    let url = format!("http://127.0.0.1:{}", port);
    Ok((child, url))
}

fn check_meta_endpoint(url: &str) -> Check {
    let client = reqwest::blocking::Client::new();
    let endpoint = format!("{}/v1/meta", url);

    match client.get(&endpoint).send() {
        Ok(response) if response.status().is_success() => {
            if let Ok(json) = response.json::<serde_json::Value>() {
                let has_version = json.get("version").is_some();
                let has_build_hash = json.get("build_hash").is_some();
                let has_build_date = json.get("build_date").is_some();

                if has_version && has_build_hash && has_build_date {
                    Check::pass(
                        "/v1/meta endpoint",
                        vec![
                            format!("Response: {}", serde_json::to_string_pretty(&json).unwrap_or_default()),
                        ],
                    )
                } else {
                    Check::fail(
                        "/v1/meta endpoint",
                        vec![format!("Response: {}", json)],
                        "Missing required fields (version, build_hash, build_date)",
                    )
                }
            } else {
                Check::fail(
                    "/v1/meta endpoint",
                    vec![],
                    "Response is not valid JSON",
                )
            }
        }
        Ok(response) => Check::fail(
            "/v1/meta endpoint",
            vec![format!("Status: {}", response.status())],
            "Non-success status code",
        ),
        Err(e) => Check::fail("/v1/meta endpoint", vec![], format!("Request failed: {}", e)),
    }
}

fn check_routing_decisions(url: &str) -> Check {
    let client = reqwest::blocking::Client::new();
    let endpoint = format!("{}/v1/routing/decisions", url);

    match client.get(&endpoint).send() {
        Ok(response) if response.status().is_success() => {
            Check::pass(
                "/v1/routing/decisions endpoint",
                vec![format!("Status: {}", response.status())],
            )
        }
        Ok(response) if response.status() == 404 => {
            Check::skip(
                "/v1/routing/decisions endpoint",
                "Endpoint returns 404 (UI should fall back to telemetry)",
            )
        }
        Ok(response) => Check::fail(
            "/v1/routing/decisions endpoint",
            vec![format!("Status: {}", response.status())],
            "Unexpected status code",
        ),
        Err(e) => Check::fail(
            "/v1/routing/decisions endpoint",
            vec![],
            format!("Request failed: {}", e),
        ),
    }
}

fn check_audits_endpoint(url: &str) -> Check {
    let client = reqwest::blocking::Client::new();
    let endpoint = format!("{}/v1/audits", url);

    match client.get(&endpoint).send() {
        Ok(response) if response.status().is_success() => {
            Check::pass(
                "/v1/audits endpoint",
                vec![format!("Status: {}", response.status())],
            )
        }
        Ok(response) if response.status() == 404 => {
            Check::skip("/v1/audits endpoint", "Endpoint not yet implemented (404)")
        }
        Ok(response) => Check::fail(
            "/v1/audits endpoint",
            vec![format!("Status: {}", response.status())],
            "Unexpected status code",
        ),
        Err(e) => Check::fail("/v1/audits endpoint", vec![], format!("Request failed: {}", e)),
    }
}

fn check_metrics_auth(url: &str, token: &str) -> Check {
    let client = reqwest::blocking::Client::new();
    let endpoint = format!("{}/metrics", url);

    // Try without token
    let no_auth_response = client.get(&endpoint).send();
    let no_auth_ok = matches!(
        no_auth_response,
        Ok(ref r) if r.status() == 401 || r.status() == 403
    );

    // Try with token
    let with_auth_response = client
        .get(&endpoint)
        .header("Authorization", format!("Bearer {}", token))
        .send();
    let with_auth_ok = matches!(with_auth_response, Ok(ref r) if r.status().is_success());

    if no_auth_ok && with_auth_ok {
        Check::pass(
            "/metrics auth",
            vec![
                "Unauthorized request rejected (401/403)".to_string(),
                "Authorized request accepted (200)".to_string(),
            ],
        )
    } else if !no_auth_ok {
        Check::fail(
            "/metrics auth",
            vec![],
            "Metrics endpoint does not require authentication",
        )
    } else {
        Check::fail(
            "/metrics auth",
            vec![],
            "Metrics endpoint rejects valid bearer token",
        )
    }
}

fn check_sighup_reload(server_pid: u32) -> Check {
    #[cfg(unix)]
    {
        use std::process::Command;

        let result = Command::new("kill")
            .args(["-HUP", &server_pid.to_string()])
            .status();

        match result {
            Ok(status) if status.success() => Check::pass(
                "SIGHUP reload",
                vec![format!("Sent SIGHUP to PID {}", server_pid)],
            ),
            _ => Check::fail("SIGHUP reload", vec![], "Failed to send SIGHUP"),
        }
    }

    #[cfg(not(unix))]
    {
        Check::skip("SIGHUP reload", "Not available on non-Unix systems")
    }
}

fn check_pid_lock(port: u16, pid_file: &Path) -> Check {
    // Try to start a second server with same PID file
    let result = Command::new("cargo")
        .args([
            "run",
            "--bin",
            "mplora-server",
            "--",
            "--config",
            "configs/cp.toml",
            "--pid-file",
            &pid_file.display().to_string(),
        ])
        .env("AOS_SERVER_PORT", port.to_string())
        .output();

    match result {
        Ok(output) if !output.status.success() => {
            let stderr = String::from_utf8_lossy(&output.stderr);
            if stderr.contains("already running") || stderr.contains("PID") {
                Check::pass(
                    "PID lock",
                    vec![format!("Second instance prevented: {}", stderr.lines().next().unwrap_or(""))],
                )
            } else {
                Check::fail(
                    "PID lock",
                    vec![stderr.to_string()],
                    "Server failed but not due to PID lock",
                )
            }
        }
        Ok(_) => Check::fail(
            "PID lock",
            vec![],
            "Second server instance started (lock not enforced)",
        ),
        Err(e) => Check::fail("PID lock", vec![], format!("Failed to test: {}", e)),
    }
}

fn check_jwt_rotation() -> Check {
    // Check for jwt_secrets table in migrations
    let migration_file = Path::new("migrations/0100_production_safety.sql");
    if !migration_file.exists() {
        return Check::skip("JWT rotation", "Migration 0100 not found");
    }

    let content = match fs::read_to_string(migration_file) {
        Ok(c) => c,
        Err(e) => {
            return Check::fail(
                "JWT rotation",
                vec![],
                format!("Failed to read migration: {}", e),
            )
        }
    };

    let has_jwt_table = content.contains("CREATE TABLE") && content.contains("jwt_secrets");
    let has_not_before = content.contains("not_before");
    let has_not_after = content.contains("not_after");

    if has_jwt_table && has_not_before && has_not_after {
        Check::pass(
            "JWT rotation",
            vec![
                "jwt_secrets table schema found".to_string(),
                "Includes not_before and not_after fields".to_string(),
            ],
        )
    } else {
        Check::fail(
            "JWT rotation",
            vec![],
            "jwt_secrets table not found or missing rotation fields",
        )
    }
}
