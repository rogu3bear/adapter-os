use std::sync::{Mutex, MutexGuard, OnceLock};

static ENV_LOCK: OnceLock<Mutex<()>> = OnceLock::new();

fn env_lock() -> MutexGuard<'static, ()> {
    let lock = ENV_LOCK.get_or_init(|| Mutex::new(()));
    match lock.lock() {
        Ok(guard) => guard,
        Err(poisoned) => poisoned.into_inner(),
    }
}

pub(crate) struct TestEnvGuard {
    _lock: MutexGuard<'static, ()>,
}

impl TestEnvGuard {
    pub(crate) fn new() -> Self {
        let lock = env_lock();
        Self { _lock: lock }
    }
}
