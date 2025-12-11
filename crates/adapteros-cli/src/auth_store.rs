use crate::output::OutputWriter;
use anyhow::{Context, Result};
use dirs::home_dir;
use serde::{Deserialize, Serialize};
use std::{env, fs, path::PathBuf};
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct AuthStore {
    pub base_url: String,
    pub tenant_id: String,
    pub token: String,
    #[serde(default)]
    pub refresh_token: Option<String>,
    pub expires_at: Option<i64>,
}

fn auth_file_path() -> Result<PathBuf> {
    if let Ok(path) = env::var("AOSCTL_AUTH_PATH") {
        return Ok(PathBuf::from(path));
    }

    let home = home_dir().context("HOME directory not set")?;
    Ok(home.join(".adapteros").join("auth.json"))
}

pub fn load_auth() -> Result<Option<AuthStore>> {
    let path = auth_file_path()?;
    if !path.exists() {
        return Ok(None);
    }

    let data = fs::read_to_string(&path)
        .with_context(|| format!("Failed to read auth store at {}", path.display()))?;
    let store: AuthStore =
        serde_json::from_str(&data).context("Failed to parse auth store JSON")?;
    Ok(Some(store))
}

pub fn save_auth(store: &AuthStore) -> Result<()> {
    let path = auth_file_path()?;
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)
            .with_context(|| format!("Failed to create auth directory {}", parent.display()))?;
    }
    let json = serde_json::to_string_pretty(store)?;
    fs::write(&path, json).with_context(|| format!("Failed to write auth store {}", path.display()))
}

pub fn clear_auth() -> Result<()> {
    let path = auth_file_path()?;
    if path.exists() {
        fs::remove_file(&path)
            .with_context(|| format!("Failed to remove auth store {}", path.display()))?;
    }
    Ok(())
}

/// Set an environment variable, scoping the unsafe call narrowly.
fn set_env(key: &str, value: &str) {
    // Safety: invoked during CLI initialization/tests; no concurrent env mutation.
    unsafe { env::set_var(key, value) };
}

/// Remove an environment variable, scoping the unsafe call narrowly.
#[cfg(test)]
fn remove_env(key: &str) {
    // Safety: invoked in single-threaded test setup/teardown.
    unsafe { env::remove_var(key) };
}

/// Set an environment variable only if it is currently unset.
/// On some targets (e.g., WASI) `env::set_var` is marked `unsafe`; we scope the
/// unsafe block narrowly and use it only during CLI startup.
fn set_env_if_absent(key: &str, value: &str) {
    if env::var(key).is_err() {
        set_env(key, value);
    }
}

/// Preload environment variables from stored login for CLI defaults.
/// This allows clap `env` defaults (AOS_TOKEN, AOS_SERVER_URL, AOS_TENANT_ID)
/// to pick up persisted credentials without explicit flags.
pub fn preload_env_from_store() {
    if env::var("AOS_TOKEN").is_ok() && env::var("AOS_SERVER_URL").is_ok() {
        return;
    }

    if let Ok(Some(store)) = load_auth() {
        set_env_if_absent("AOS_TOKEN", &store.token);
        set_env_if_absent("AOS_SERVER_URL", &store.base_url);
        set_env_if_absent("AOS_TENANT_ID", &store.tenant_id);
    }
}

/// Emit a warning when a command-supplied tenant differs from stored tenant.
/// This is a soft guard to catch accidental cross-tenant usage; admins can ignore it.
pub fn warn_if_tenant_mismatch(request_tenant: Option<&str>, output: &OutputWriter) {
    if let (Some(req_tid), Ok(Some(store))) = (request_tenant, load_auth()) {
        if !req_tid.is_empty() && store.tenant_id != req_tid {
            let message = format!(
                "Tenant mismatch: stored tenant '{}' vs requested '{}'. If intentional (admin), continue; otherwise re-run login or use --tenant-id.",
                store.tenant_id, req_tid
            );
            output.warning(&message);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serial_test::serial;
    use tempfile::TempDir;

    fn with_temp_store() -> (TempDir, PathBuf) {
        let dir = TempDir::new().expect("tmpdir");
        let path = dir.path().join("auth.json");
        set_env("AOSCTL_AUTH_PATH", path.to_string_lossy().as_ref());
        (dir, path)
    }

    #[test]
    #[serial]
    fn roundtrip_store() {
        let (_dir, path) = with_temp_store();
        let store = AuthStore {
            base_url: "http://localhost:8080".to_string(),
            tenant_id: "tenant-a".to_string(),
            token: "abc123".to_string(),
            refresh_token: Some("refresh123".to_string()),
            expires_at: Some(100),
        };
        save_auth(&store).expect("save");
        let loaded = load_auth().expect("load").expect("some");
        assert_eq!(store, loaded);
        assert!(path.exists());
        remove_env("AOSCTL_AUTH_PATH");
    }

    #[test]
    #[serial]
    fn preload_sets_env_when_missing() {
        let (_dir, _path) = with_temp_store();
        let store = AuthStore {
            base_url: "http://env.example".to_string(),
            tenant_id: "tenant-env".to_string(),
            token: "token-env".to_string(),
            refresh_token: Some("refresh-env".to_string()),
            expires_at: None,
        };
        save_auth(&store).expect("save");

        remove_env("AOS_TOKEN");
        remove_env("AOS_SERVER_URL");
        remove_env("AOS_TENANT_ID");

        preload_env_from_store();

        assert_eq!(env::var("AOS_TOKEN").unwrap(), "token-env");
        assert_eq!(env::var("AOS_SERVER_URL").unwrap(), "http://env.example");
        assert_eq!(env::var("AOS_TENANT_ID").unwrap(), "tenant-env");
        remove_env("AOS_TOKEN");
        remove_env("AOS_SERVER_URL");
        remove_env("AOS_TENANT_ID");
        remove_env("AOSCTL_AUTH_PATH");
    }
}
