//! Service registry for boot-time service management.
//!
//! The ServiceRegistry provides a simple way to track services that are
//! initialized during boot and need to be available throughout the application
//! lifecycle.
//!
//! ## Design Philosophy
//!
//! This registry is intentionally simple and does NOT depend on Axum types.
//! It stores type-erased handles that can be downcast by the caller.
//! The actual service creation and management happens in the LifecycleBuilder
//! or RouterBuilder, not here.

use std::any::{Any, TypeId};
use std::collections::HashMap;
use std::sync::Arc;

/// A registry for services initialized during boot.
///
/// Services are stored as type-erased `Arc<dyn Any + Send + Sync>` and can be
/// retrieved by their type. This allows the boot crate to manage services
/// without depending on their concrete types.
#[derive(Default)]
pub struct ServiceRegistry {
    services: HashMap<TypeId, Arc<dyn Any + Send + Sync>>,
    named_services: HashMap<String, Arc<dyn Any + Send + Sync>>,
}

impl ServiceRegistry {
    /// Create a new empty service registry.
    pub fn new() -> Self {
        Self::default()
    }

    /// Register a service by its type.
    ///
    /// If a service of the same type was already registered, it is replaced.
    pub fn register<T: Any + Send + Sync + 'static>(&mut self, service: T) {
        self.services.insert(TypeId::of::<T>(), Arc::new(service));
    }

    /// Register a service by its type, wrapped in Arc.
    pub fn register_arc<T: Any + Send + Sync + 'static>(&mut self, service: Arc<T>) {
        self.services.insert(TypeId::of::<T>(), service);
    }

    /// Get a service by its type.
    ///
    /// Returns None if the service was not registered.
    pub fn get<T: Any + Send + Sync + 'static>(&self) -> Option<Arc<T>> {
        self.services
            .get(&TypeId::of::<T>())
            .and_then(|s| Arc::clone(s).downcast::<T>().ok())
    }

    /// Check if a service of the given type is registered.
    pub fn contains<T: Any + Send + Sync + 'static>(&self) -> bool {
        self.services.contains_key(&TypeId::of::<T>())
    }

    /// Register a service by name.
    ///
    /// Named services are useful when you need to register multiple instances
    /// of the same type (e.g., different database connections).
    pub fn register_named<T: Any + Send + Sync + 'static>(
        &mut self,
        name: impl Into<String>,
        service: T,
    ) {
        self.named_services.insert(name.into(), Arc::new(service));
    }

    /// Get a named service.
    pub fn get_named<T: Any + Send + Sync + 'static>(&self, name: &str) -> Option<Arc<T>> {
        self.named_services
            .get(name)
            .and_then(|s| Arc::clone(s).downcast::<T>().ok())
    }

    /// Get the number of registered services (by type).
    pub fn len(&self) -> usize {
        self.services.len()
    }

    /// Check if the registry is empty.
    pub fn is_empty(&self) -> bool {
        self.services.is_empty() && self.named_services.is_empty()
    }

    /// Get the number of named services.
    pub fn named_len(&self) -> usize {
        self.named_services.len()
    }
}

impl std::fmt::Debug for ServiceRegistry {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ServiceRegistry")
            .field("services_count", &self.services.len())
            .field("named_services_count", &self.named_services.len())
            .finish()
    }
}

/// A marker trait for services that can be registered in the ServiceRegistry.
///
/// This is a convenience trait that combines the required bounds.
pub trait Service: Any + Send + Sync + 'static {}

impl<T: Any + Send + Sync + 'static> Service for T {}

#[cfg(test)]
mod tests {
    use super::*;

    struct TestService {
        value: i32,
    }

    struct AnotherService {
        name: String,
    }

    #[test]
    fn test_register_and_get() {
        let mut registry = ServiceRegistry::new();
        registry.register(TestService { value: 42 });

        let service = registry.get::<TestService>();
        assert!(service.is_some());
        assert_eq!(service.unwrap().value, 42);
    }

    #[test]
    fn test_get_unregistered() {
        let registry = ServiceRegistry::new();
        let service = registry.get::<TestService>();
        assert!(service.is_none());
    }

    #[test]
    fn test_multiple_services() {
        let mut registry = ServiceRegistry::new();
        registry.register(TestService { value: 1 });
        registry.register(AnotherService {
            name: "test".into(),
        });

        assert_eq!(registry.get::<TestService>().unwrap().value, 1);
        assert_eq!(registry.get::<AnotherService>().unwrap().name, "test");
    }

    #[test]
    fn test_named_services() {
        let mut registry = ServiceRegistry::new();
        registry.register_named("primary", TestService { value: 1 });
        registry.register_named("secondary", TestService { value: 2 });

        assert_eq!(
            registry.get_named::<TestService>("primary").unwrap().value,
            1
        );
        assert_eq!(
            registry
                .get_named::<TestService>("secondary")
                .unwrap()
                .value,
            2
        );
    }

    #[test]
    fn test_contains() {
        let mut registry = ServiceRegistry::new();
        assert!(!registry.contains::<TestService>());

        registry.register(TestService { value: 0 });
        assert!(registry.contains::<TestService>());
    }

    #[test]
    fn test_replace_service() {
        let mut registry = ServiceRegistry::new();
        registry.register(TestService { value: 1 });
        registry.register(TestService { value: 2 });

        assert_eq!(registry.get::<TestService>().unwrap().value, 2);
        assert_eq!(registry.len(), 1);
    }
}
