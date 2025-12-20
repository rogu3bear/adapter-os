use crate::auth_store::{load_auth, save_auth, AuthStore};
use anyhow::{anyhow, bail, Context, Result};
use reqwest::{header, Client, RequestBuilder, StatusCode};
use serde::{Deserialize, Serialize};

#[derive(Debug, Deserialize, Serialize)]
struct RefreshResponseBody {
    token: String,
    expires_at: i64,
}

/// Extract a cookie value from a response header map.
pub fn extract_cookie(headers: &header::HeaderMap, name: &str) -> Option<String> {
    headers
        .get_all(header::SET_COOKIE)
        .iter()
        .find_map(|value| {
            value.to_str().ok().and_then(|raw| {
                raw.split(';')
                    .find_map(|part| part.trim().strip_prefix(&format!("{name}=")))
                    .map(str::to_string)
            })
        })
}

/// Refresh access and refresh tokens using the stored refresh_token.
pub async fn refresh_tokens(client: &Client, auth: &mut AuthStore) -> Result<()> {
    let refresh_token = auth
        .refresh_token
        .as_ref()
        .ok_or_else(|| anyhow!("refresh token missing"))?
        .clone();
    let url = format!("{}/v1/auth/refresh", auth.base_url.trim_end_matches('/'));
    let resp = client
        .post(&url)
        .header(header::COOKIE, format!("refresh_token={refresh_token}"))
        .send()
        .await
        .context("refresh request failed")?;

    if resp.status() != StatusCode::OK {
        bail!("refresh failed with status {}", resp.status());
    }

    let refresh_cookie = extract_cookie(resp.headers(), "refresh_token");
    let body: RefreshResponseBody = resp.json().await.context("parse refresh response")?;

    auth.token = body.token;
    auth.refresh_token = refresh_cookie.or(auth.refresh_token.take());
    auth.expires_at = Some(body.expires_at);
    save_auth(auth)?;
    Ok(())
}

/// Send a request with bearer auth, retrying once on 401 via refresh.
pub async fn send_with_refresh<F>(
    client: &Client,
    auth: &mut AuthStore,
    build: F,
) -> Result<reqwest::Response>
where
    F: Fn(&Client, &str) -> RequestBuilder,
{
    let mut resp = build(client, &auth.token).send().await?;
    if resp.status() != StatusCode::UNAUTHORIZED {
        return Ok(resp);
    }

    refresh_tokens(client, auth).await?;
    resp = build(client, &auth.token).send().await?;
    Ok(resp)
}

/// Load auth state and send a request with auto-refresh when possible.
pub async fn send_with_refresh_from_store<F>(client: &Client, build: F) -> Result<reqwest::Response>
where
    F: Fn(&Client, &mut AuthStore) -> RequestBuilder,
{
    let mut auth =
        load_auth()?.ok_or_else(|| anyhow!("no stored auth; run `aosctl auth login`"))?;

    let mut resp = build(client, &mut auth).send().await?;
    if resp.status() != StatusCode::UNAUTHORIZED {
        return Ok(resp);
    }

    refresh_tokens(client, &mut auth).await?;
    resp = build(client, &mut auth).send().await?;
    Ok(resp)
}

#[cfg(test)]
mod tests {
    use super::*;
    use adapteros_platform::common::PlatformUtils;
    use axum::{
        body::Body,
        extract::State,
        http::{Request, StatusCode as AxumStatus},
        response::IntoResponse,
        routing::{get, post},
        Router,
    };
    use std::{net::SocketAddr, sync::Arc};
    use tempfile::TempDir;
    use tokio::net::TcpListener;
    use tokio::task::JoinHandle;

    fn new_test_tempdir() -> TempDir {
        let root = PlatformUtils::temp_dir();
        std::fs::create_dir_all(&root).expect("create var/tmp");
        TempDir::new_in(&root).expect("create temp dir")
    }

    #[derive(Clone)]
    struct ServerState {
        expected_refresh: String,
        refreshed_token: String,
    }

    fn start_server(state: ServerState) -> (String, JoinHandle<()>) {
        let app = Router::new()
            .route(
                "/target",
                post(
                    |State(state): State<Arc<ServerState>>, req: Request<Body>| async move {
                        let auth = req
                            .headers()
                            .get(header::AUTHORIZATION)
                            .and_then(|v| v.to_str().ok())
                            .unwrap_or_default()
                            .to_string();
                        if auth == "Bearer old-token" {
                            return AxumStatus::UNAUTHORIZED.into_response();
                        }
                        if auth == format!("Bearer {}", state.refreshed_token) {
                            return AxumStatus::OK.into_response();
                        }
                        AxumStatus::UNAUTHORIZED.into_response()
                    },
                ),
            )
            .route(
                "/v1/auth/refresh",
                post(
                    |State(state): State<Arc<ServerState>>, req: Request<Body>| async move {
                        let cookie = req
                            .headers()
                            .get(header::COOKIE)
                            .and_then(|v| v.to_str().ok())
                            .unwrap_or_default()
                            .to_string();
                        if !cookie.contains(&format!("refresh_token={}", state.expected_refresh)) {
                            return AxumStatus::UNAUTHORIZED.into_response();
                        }
                        (
                            [(
                                header::SET_COOKIE,
                                format!(
                                    "refresh_token=new-refresh; Path=/; Max-Age=3600; SameSite=Lax"
                                ),
                            )],
                            serde_json::to_string(&RefreshResponseBody {
                                token: state.refreshed_token.clone(),
                                expires_at: Utc::now().timestamp() + 3600,
                            })
                            .unwrap(),
                        )
                            .into_response()
                    },
                ),
            )
            .with_state(Arc::new(state));

        let std_listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        std_listener.set_nonblocking(true).unwrap();
        let addr: SocketAddr = std_listener.local_addr().unwrap();
        let listener = TcpListener::from_std(std_listener).unwrap();
        let handle = tokio::spawn(async move {
            axum::serve(listener, app).await.unwrap();
        });
        (format!("http://{}", addr), handle)
    }

    #[tokio::test]
    async fn send_with_refresh_retries_on_401() {
        let temp = new_test_tempdir();
        let auth_path = temp.path().join("auth.json");
        std::env::set_var("AOSCTL_AUTH_PATH", &auth_path);

        let (base_url, handle) = start_server(ServerState {
            expected_refresh: "refresh-123".to_string(),
            refreshed_token: "new-token".to_string(),
        });

        let mut auth = AuthStore {
            base_url: base_url.clone(),
            tenant_id: "tenant".to_string(),
            token: "old-token".to_string(),
            refresh_token: Some("refresh-123".to_string()),
            expires_at: None,
        };
        save_auth(&auth).unwrap();

        let client = Client::builder().build().unwrap();
        let resp = send_with_refresh(&client, &mut auth, |client, token| {
            client
                .post(format!("{}/target", base_url))
                .bearer_auth(token)
        })
        .await
        .expect("request should succeed");

        assert_eq!(resp.status(), StatusCode::OK);
        assert_eq!(auth.token, "new-token");
        assert_eq!(auth.refresh_token.as_deref(), Some("new-refresh"));

        handle.abort();
    }

    #[tokio::test]
    async fn send_with_refresh_fails_when_refresh_denied() {
        let temp = new_test_tempdir();
        let auth_path = temp.path().join("auth.json");
        std::env::set_var("AOSCTL_AUTH_PATH", &auth_path);

        let (base_url, handle) = start_server(ServerState {
            expected_refresh: "other".to_string(),
            refreshed_token: "new-token".to_string(),
        });

        let mut auth = AuthStore {
            base_url: base_url.clone(),
            tenant_id: "tenant".to_string(),
            token: "old-token".to_string(),
            refresh_token: Some("refresh-123".to_string()),
            expires_at: None,
        };
        save_auth(&auth).unwrap();

        let client = Client::builder().build().unwrap();
        let resp = send_with_refresh(&client, &mut auth, |client, token| {
            client
                .post(format!("{}/target", base_url))
                .bearer_auth(token)
        })
        .await;

        assert!(resp.is_err(), "refresh failure should error");
        assert_eq!(auth.token, "old-token");

        handle.abort();
    }
}
