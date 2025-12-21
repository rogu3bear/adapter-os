use std::collections::HashMap;
use std::sync::{Mutex, MutexGuard, OnceLock};

static ENV_LOCK: OnceLock<Mutex<()>> = OnceLock::new();

fn env_lock() -> MutexGuard<'static, ()> {
    let lock = ENV_LOCK.get_or_init(|| Mutex::new(()));
    match lock.lock() {
        Ok(guard) => guard,
        Err(poisoned) => poisoned.into_inner(),
    }
}

fn clear_test_env_vars() {
    let keys: Vec<String> = std::env::vars().map(|(key, _)| key).collect();
    for key in keys {
        if key.starts_with("AOS_") || key.starts_with("ADAPTEROS_") || key == "DATABASE_URL" {
            std::env::remove_var(key);
        }
    }
}

pub struct TestEnvGuard {
    _lock: MutexGuard<'static, ()>,
    snapshot: HashMap<String, String>,
}

impl TestEnvGuard {
    pub fn new() -> Self {
        let lock = env_lock();
        let snapshot = std::env::vars().collect::<HashMap<_, _>>();
        clear_test_env_vars();
        std::env::set_var("AOS_SKIP_DOTENV", "1");
        Self {
            _lock: lock,
            snapshot,
        }
    }
}

impl Drop for TestEnvGuard {
    fn drop(&mut self) {
        let current_keys: Vec<String> = std::env::vars().map(|(key, _)| key).collect();
        for key in current_keys {
            if !self.snapshot.contains_key(&key) {
                std::env::remove_var(key);
            }
        }
        for (key, value) in self.snapshot.iter() {
            std::env::set_var(key, value);
        }
    }
}
