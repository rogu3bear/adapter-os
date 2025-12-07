use crate::auth_store::{clear_auth, load_auth, save_auth, AuthStore};
use crate::output::OutputWriter;
use adapteros_api_types::auth::LoginRequest;
use anyhow::{Context, Result};
use chrono::Utc;
use clap::Subcommand;

#[derive(Debug, Subcommand, Clone)]
pub enum AuthCommand {
    /// Log in and persist token for CLI use
    Login {
        /// Control plane base URL (default: AOS_SERVER_URL or http://localhost:8080)
        #[arg(long, env = "AOS_SERVER_URL", default_value = "http://localhost:8080")]
        server_url: String,

        /// Account email
        #[arg(long)]
        email: String,

        /// Account password
        #[arg(long)]
        password: String,

        /// Tenant to target (defaults to returned tenant)
        #[arg(long)]
        tenant_id: Option<String>,

        /// Optional device identifier
        #[arg(long)]
        device_id: Option<String>,
    },

    /// Clear stored CLI credentials
    Logout,

    /// Show stored auth state (no token printed)
    Show,
}

pub async fn handle_auth_command(cmd: AuthCommand, output: &OutputWriter) -> Result<()> {
    match cmd {
        AuthCommand::Login {
            server_url,
            email,
            password,
            tenant_id,
            device_id,
        } => {
            let base = server_url.trim_end_matches('/').to_string();
            let url = format!("{}/v1/auth/login", base);
            let req = LoginRequest {
                username: None,
                email: email.clone(),
                password,
                device_id,
                totp_code: None,
            };

            let client = reqwest::Client::new();
            let resp = client
                .post(&url)
                .json(&req)
                .send()
                .await
                .with_context(|| format!("Failed to reach {}", url))?
                .error_for_status()
                .context("Login failed")?;

            let login: adapteros_api_types::auth::LoginResponse = resp
                .json()
                .await
                .context("Failed to parse login response")?;

            let effective_tenant = tenant_id.unwrap_or_else(|| login.tenant_id.clone());
            let expires_at = Some(Utc::now().timestamp() + login.expires_in as i64);

            let store = AuthStore {
                base_url: base.clone(),
                tenant_id: effective_tenant.clone(),
                token: login.token.clone(),
                expires_at,
            };
            save_auth(&store)?;

            output.success("Login succeeded; token stored for CLI use");
            output.kv("User", &login.user_id);
            output.kv("Tenant", &effective_tenant);
            output.kv("Role", &login.role);
            output.info("Tip: tokens from auth store are used when --token is omitted.");
        }
        AuthCommand::Logout => {
            clear_auth()?;
            output.success("Cleared stored CLI credentials");
        }
        AuthCommand::Show => match load_auth()? {
            Some(store) => {
                output.section("Stored auth");
                output.kv("Base URL", &store.base_url);
                output.kv("Tenant", &store.tenant_id);
                if let Some(exp) = store.expires_at {
                    output.kv("Expires At", &exp.to_string());
                } else {
                    output.kv("Expires At", "unknown");
                }
                output.info("Token is stored but not printed for safety.");
            }
            None => {
                output.warning("No stored CLI credentials. Run: aosctl auth login");
            }
        },
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::auth_store::{
        load_auth, preload_env_from_store, save_auth, warn_if_tenant_mismatch,
    };
    use crate::output::{OutputMode, OutputWriter};
    use adapteros_api_types::auth::LoginResponse;
    use axum::{routing::post, serve, Json, Router};
    use serial_test::serial;
    use std::env;
    use std::net::SocketAddr;
    use std::sync::{Arc, Mutex};
    use tempfile::TempDir;
    use tokio::net::TcpListener;
    use tokio::task::JoinHandle;

    async fn start_mock_auth_server(body: LoginResponse) -> (String, JoinHandle<()>) {
        let app = {
            let response = body.clone();
            Router::new().route(
                "/v1/auth/login",
                post(move || {
                    let resp = response.clone();
                    async move { Json(resp.clone()) }
                }),
            )
        };

        let listener = TcpListener::bind("127.0.0.1:0")
            .await
            .expect("bind mock server");
        let addr: SocketAddr = listener.local_addr().expect("local addr");
        let handle = tokio::spawn(async move {
            if let Err(e) = serve(listener, app).await {
                eprintln!("mock auth server error: {e}");
            }
        });

        (format!("http://{}", addr), handle)
    }

    #[tokio::test]
    #[serial]
    async fn login_stores_and_preloads_env_without_leaking_token() {
        let temp = TempDir::new().expect("tmpdir");
        let auth_path = temp.path().join("auth.json");
        env::set_var("AOSCTL_AUTH_PATH", &auth_path);
        env::remove_var("AOS_TOKEN");
        env::remove_var("AOS_SERVER_URL");
        env::remove_var("AOS_TENANT_ID");
        env::remove_var("AOSCTL_AUTH_PATH");

        let login_response = LoginResponse {
            schema_version: "1".to_string(),
            token: "token-login-1".to_string(),
            user_id: "user-xyz".to_string(),
            tenant_id: "tenant-xyz".to_string(),
            role: "admin".to_string(),
            expires_in: 3600,
            tenants: None,
            mfa_level: None,
        };

        let (base_url, server_handle) = start_mock_auth_server(login_response.clone()).await;
        let sink = Arc::new(Mutex::new(Vec::new()));
        let output = OutputWriter::with_sink(OutputMode::Text, false, sink.clone());

        handle_auth_command(
            AuthCommand::Login {
                server_url: base_url.clone(),
                email: "user@example.com".to_string(),
                password: "secret".to_string(),
                tenant_id: None,
                device_id: None,
            },
            &output,
        )
        .await
        .expect("login command should succeed");

        server_handle.abort();

        let stored = load_auth().expect("load store").expect("store present");
        assert_eq!(stored.base_url, base_url);
        assert_eq!(stored.tenant_id, login_response.tenant_id);
        assert_eq!(stored.token, login_response.token);

        env::remove_var("AOS_TOKEN");
        env::remove_var("AOS_SERVER_URL");
        env::remove_var("AOS_TENANT_ID");

        preload_env_from_store();
        assert_eq!(env::var("AOS_TOKEN").unwrap(), "token-login-1");
        assert_eq!(env::var("AOS_SERVER_URL").unwrap(), base_url);
        assert_eq!(env::var("AOS_TENANT_ID").unwrap(), "tenant-xyz");

        let captured = sink.lock().unwrap().join("\n");
        assert!(
            !captured.contains("token-login-1"),
            "token must not be printed in success output"
        );
    }

    #[tokio::test]
    #[serial]
    async fn warns_on_tenant_mismatch() {
        let temp = TempDir::new().expect("tmpdir");
        let auth_path = temp.path().join("auth.json");
        env::set_var("AOSCTL_AUTH_PATH", &auth_path);

        let store = AuthStore {
            base_url: "http://example.com".to_string(),
            tenant_id: "stored-tenant".to_string(),
            token: "token-warn".to_string(),
            expires_at: None,
        };
        save_auth(&store).expect("save auth");

        let sink = Arc::new(Mutex::new(Vec::new()));
        let output = OutputWriter::with_sink(OutputMode::Text, false, sink.clone());

        warn_if_tenant_mismatch(Some("other-tenant"), &output);

        let captured = sink.lock().unwrap().join("\n");
        assert!(
            captured.contains("Tenant mismatch"),
            "expected tenant mismatch warning to be recorded"
        );
        env::remove_var("AOSCTL_AUTH_PATH");
        env::remove_var("AOS_TOKEN");
        env::remove_var("AOS_SERVER_URL");
        env::remove_var("AOS_TENANT_ID");
    }
}
