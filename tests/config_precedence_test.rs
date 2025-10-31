#![cfg(all(test, feature = "extended-tests"))]

use adapteros_config::{initialize_config, is_frozen};
use std::sync::{Mutex, MutexGuard, OnceLock};

fn env_lock() -> &'static Mutex<()> {
    static LOCK: OnceLock<Mutex<()>> = OnceLock::new();
    LOCK.get_or_init(|| Mutex::new(()))
}

fn lock_env() -> MutexGuard<'static, ()> {
    env_lock().lock().unwrap_or_else(|e| e.into_inner())
}

#[test]
fn test_initialize_config_singleton_behavior() {
    let _guard = lock_env();
    std::env::set_var("ADAPTEROS_DATABASE_URL", "sqlite://singleton.db");

    let config = initialize_config(vec!["adapteros".to_string()], None).expect("init config");
    assert!(config.is_frozen());
    assert!(is_frozen());

    let second = initialize_config(vec!["adapteros".to_string()], None);
    assert!(second.is_err());

    let global = adapteros_config::get_config().expect("global config");
    assert_eq!(
        global.get("database.url"),
        Some(&"sqlite://singleton.db".to_string())
    );

    std::env::remove_var("ADAPTEROS_DATABASE_URL");
}
