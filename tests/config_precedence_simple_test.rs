use adapteros_config::ConfigLoader;
use std::io::Write;
use std::sync::{Mutex, MutexGuard, OnceLock};
use tempfile::NamedTempFile;

fn loader() -> ConfigLoader {
    ConfigLoader::new()
}

fn env_lock() -> &'static Mutex<()> {
    static LOCK: OnceLock<Mutex<()>> = OnceLock::new();
    LOCK.get_or_init(|| Mutex::new(()))
}

fn lock_env() -> MutexGuard<'static, ()> {
    env_lock().lock().unwrap_or_else(|e| e.into_inner())
}

#[test]
fn test_cli_overrides_environment() {
    let _guard = lock_env();
    std::env::set_var("ADAPTEROS_DATABASE_URL", "sqlite://env.db");
    std::env::set_var("ADAPTEROS_APP_NAME", "env-app");

    let manifest = {
        let mut file = NamedTempFile::new().unwrap();
        write!(
            file,
            r#"
[app]
name = "manifest-app"
"#
        )
        .unwrap();
        file.flush().unwrap();
        file
    };

    let cli_args = vec![
        "adapteros".to_string(),
        "--adapteros-app-name".to_string(),
        "cli-app".to_string(),
    ];

    let config = loader()
        .load(
            cli_args,
            Some(manifest.path().to_string_lossy().to_string()),
        )
        .expect("load config");

    assert_eq!(config.get("app.name"), Some(&"cli-app".to_string()));

    std::env::remove_var("ADAPTEROS_APP_NAME");
    std::env::remove_var("ADAPTEROS_DATABASE_URL");
}

#[test]
fn test_missing_required_field_fails_validation() {
    let _guard = lock_env();
    let manifest = {
        let mut file = NamedTempFile::new().unwrap();
        write!(
            file,
            r#"
[server]
host = "127.0.0.1"
"#
        )
        .unwrap();
        file.flush().unwrap();
        file
    };

    let result = loader().load(
        vec!["adapteros".to_string()],
        Some(manifest.path().to_string_lossy().to_string()),
    );
    assert!(result.is_err());
    let message = result.unwrap_err().to_string();
    assert!(message.contains("database.url"));
}
