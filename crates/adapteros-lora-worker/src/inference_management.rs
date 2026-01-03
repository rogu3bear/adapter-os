use std::collections::HashMap;
use std::sync::{
    atomic::{AtomicBool, Ordering},
    Arc,
};

use parking_lot::{Mutex, RwLock};

#[derive(Debug)]
pub struct InferenceCancelToken {
    cancelled: AtomicBool,
    reason: Mutex<Option<String>>,
}

impl InferenceCancelToken {
    pub fn new() -> Self {
        Self {
            cancelled: AtomicBool::new(false),
            reason: Mutex::new(None),
        }
    }

    pub fn cancel(&self, reason: Option<String>) {
        if let Some(reason) = reason {
            let mut guard = self.reason.lock();
            if guard.is_none() {
                *guard = Some(reason);
            }
        }
        self.cancelled.store(true, Ordering::Release);
    }

    pub fn is_cancelled(&self) -> bool {
        self.cancelled.load(Ordering::Acquire)
    }

    pub fn reason(&self) -> Option<String> {
        self.reason.lock().clone()
    }
}

#[derive(Debug, Default)]
pub struct InferenceCancelRegistry {
    requests: RwLock<HashMap<String, Arc<InferenceCancelToken>>>,
}

impl InferenceCancelRegistry {
    pub fn new() -> Self {
        Self {
            requests: RwLock::new(HashMap::new()),
        }
    }

    pub fn register(&self, request_id: &str) -> Arc<InferenceCancelToken> {
        let mut guard = self.requests.write();
        guard
            .entry(request_id.to_string())
            .or_insert_with(|| Arc::new(InferenceCancelToken::new()))
            .clone()
    }

    pub fn unregister(&self, request_id: &str) {
        let mut guard = self.requests.write();
        guard.remove(request_id);
    }

    pub fn cancel(&self, request_id: &str, reason: Option<String>) -> bool {
        let guard = self.requests.read();
        if let Some(token) = guard.get(request_id) {
            token.cancel(reason);
            true
        } else {
            false
        }
    }
}

pub struct InferenceCancelGuard {
    registry: Arc<InferenceCancelRegistry>,
    request_id: String,
}

impl InferenceCancelGuard {
    pub fn new(registry: Arc<InferenceCancelRegistry>, request_id: String) -> Self {
        Self {
            registry,
            request_id,
        }
    }
}

impl Drop for InferenceCancelGuard {
    fn drop(&mut self) {
        self.registry.unregister(&self.request_id);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cancel_records_reason_and_flag() {
        let registry = InferenceCancelRegistry::new();
        let token = registry.register("req-1");

        assert!(!token.is_cancelled());
        assert!(registry.cancel("req-1", Some("client_disconnect".to_string())));
        assert!(token.is_cancelled());
        assert_eq!(token.reason().as_deref(), Some("client_disconnect"));

        registry.unregister("req-1");
    }
}
