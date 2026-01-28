//! Middleware ordering enforcement tests
//!
//! These tests verify that the type-state pattern correctly enforces
//! middleware ordering at compile time.
//!
//! # Security Rationale
//!
//! Middleware ordering is critical for security. The correct order is:
//!
//! 1. **Auth** - Validates credentials, sets Claims/Principal
//! 2. **TenantGuard** - Validates tenant isolation (requires Claims)
//! 3. **CSRF** - Validates CSRF tokens (requires auth context)
//! 4. **Context** - Consolidates auth data
//! 5. **Policy** - Enforces policies (requires context)
//! 6. **Audit** - Logs operations (requires context)
//!
//! If middleware runs out of order, security can be bypassed:
//! - Tenant guard before auth: No claims to check, bypass isolation
//! - Policy before context: No context for policy evaluation
//! - CSRF before auth: Cannot determine if CSRF check needed
//!
//! # Compile-Time Enforcement
//!
//! The type-state pattern ensures incorrect ordering is a compile error.
//! The tests below verify that correct orderings compile successfully.
//!
//! ## Incorrect Orderings (Would Not Compile)
//!
//! These examples show what WOULD NOT compile if attempted:
//!
//! ```compile_fail
//! use adapteros_server_api::middleware::chain_builder::ProtectedMiddlewareChain;
//!
//! // ERROR: Cannot skip auth - no method `with_tenant_guard` on NeedsAuth state
//! let _chain = ProtectedMiddlewareChain::new()
//!     .with_tenant_guard()  // Compile error!
//!     .with_csrf()
//!     .with_context()
//!     .with_policy()
//!     .with_audit()
//!     .build();
//! ```
//!
//! ```compile_fail
//! use adapteros_server_api::middleware::chain_builder::ProtectedMiddlewareChain;
//!
//! // ERROR: Cannot do CSRF before tenant guard
//! let _chain = ProtectedMiddlewareChain::new()
//!     .with_required_auth()
//!     .with_csrf()  // Compile error! Expected NeedsTenantGuard state
//!     .with_tenant_guard()
//!     .with_context()
//!     .with_policy()
//!     .with_audit()
//!     .build();
//! ```
//!
//! ```compile_fail
//! use adapteros_server_api::middleware::chain_builder::ProtectedMiddlewareChain;
//!
//! // ERROR: Cannot skip context
//! let _chain = ProtectedMiddlewareChain::new()
//!     .with_required_auth()
//!     .with_tenant_guard()
//!     .with_csrf()
//!     .with_policy()  // Compile error! Expected NeedsContext state
//!     .with_audit()
//!     .build();
//! ```
//!
//! ```compile_fail
//! use adapteros_server_api::middleware::chain_builder::ProtectedMiddlewareChain;
//!
//! // ERROR: Cannot build incomplete chain
//! let _chain = ProtectedMiddlewareChain::new()
//!     .with_required_auth()
//!     .with_tenant_guard()
//!     .with_csrf()
//!     .with_context()
//!     .build();  // Compile error! No method `build` on NeedsPolicy state
//! ```

use adapteros_server_api::middleware::chain_builder::{
    api_key_chain, internal_chain, optional_auth_chain, protected_chain, AuditMiddleware,
    AuthMiddleware, ContextMiddleware, CsrfMiddleware, PolicyMiddleware, ProtectedMiddlewareChain,
    TenantGuardMiddleware,
};

/// Test that the full protected chain compiles with correct ordering.
#[test]
fn test_protected_chain_correct_ordering() {
    let config = ProtectedMiddlewareChain::new()
        .with_required_auth()
        .with_tenant_guard()
        .with_csrf()
        .with_context()
        .with_policy()
        .with_audit()
        .build();

    // Verify all middleware is configured correctly
    assert!(matches!(config.auth, AuthMiddleware::Required));
    assert!(matches!(
        config.tenant_guard,
        TenantGuardMiddleware::Standard
    ));
    assert!(matches!(config.csrf, CsrfMiddleware::Standard));
    assert!(matches!(config.context, ContextMiddleware::Standard));
    assert!(matches!(config.policy, PolicyMiddleware::Standard));
    assert!(matches!(config.audit, AuditMiddleware::Standard));

    // Verify description
    let desc = config.describe();
    assert!(desc.contains("auth(required)"));
    assert!(desc.contains("tenant_guard"));
    assert!(desc.contains("csrf"));
    assert!(desc.contains("context"));
    assert!(desc.contains("policy"));
    assert!(desc.contains("audit"));

    // Verify helper methods
    assert!(config.requires_auth());
    assert!(config.enforces_tenant_guard());
    assert!(config.enforces_csrf());
    assert!(config.enforces_policy());
    assert!(config.performs_audit());
}

/// Test optional auth chain compiles correctly.
#[test]
fn test_optional_auth_chain_correct_ordering() {
    let config = ProtectedMiddlewareChain::new()
        .with_optional_auth()
        .skip_tenant_guard()
        .skip_csrf()
        .with_context()
        .skip_policy()
        .skip_audit()
        .build();

    assert!(matches!(config.auth, AuthMiddleware::Optional));
    assert!(matches!(config.tenant_guard, TenantGuardMiddleware::Skip));
    assert!(matches!(config.csrf, CsrfMiddleware::Skip));
    assert!(matches!(config.context, ContextMiddleware::Standard));
    assert!(matches!(config.policy, PolicyMiddleware::Skip));
    assert!(matches!(config.audit, AuditMiddleware::Skip));

    // Optional auth doesn't "require" auth (won't reject unauthenticated)
    assert!(!config.requires_auth());
    assert!(!config.enforces_tenant_guard());
    assert!(!config.enforces_csrf());
    assert!(!config.enforces_policy());
    assert!(!config.performs_audit());
}

/// Test dual auth chain compiles correctly.
#[test]
fn test_dual_auth_chain_correct_ordering() {
    let config = ProtectedMiddlewareChain::new()
        .with_dual_auth()
        .with_tenant_guard()
        .with_csrf()
        .with_context()
        .with_policy()
        .with_audit()
        .build();

    assert!(matches!(config.auth, AuthMiddleware::Dual));
    assert!(config.requires_auth());
}

/// Test that skip variants work at each stage.
#[test]
fn test_skip_variants_compile() {
    // All skips (except context which has no skip)
    let config = ProtectedMiddlewareChain::new()
        .with_required_auth()
        .skip_tenant_guard()
        .skip_csrf()
        .with_context()
        .skip_policy()
        .skip_audit()
        .build();

    assert!(matches!(config.tenant_guard, TenantGuardMiddleware::Skip));
    assert!(matches!(config.csrf, CsrfMiddleware::Skip));
    assert!(matches!(config.policy, PolicyMiddleware::Skip));
    assert!(matches!(config.audit, AuditMiddleware::Skip));
}

/// Test convenience constructors produce expected configurations.
#[test]
fn test_convenience_constructors() {
    // protected_chain()
    let protected = protected_chain();
    assert!(matches!(protected.auth, AuthMiddleware::Required));
    assert!(matches!(
        protected.tenant_guard,
        TenantGuardMiddleware::Standard
    ));
    assert!(matches!(protected.csrf, CsrfMiddleware::Standard));
    assert!(matches!(protected.context, ContextMiddleware::Standard));
    assert!(matches!(protected.policy, PolicyMiddleware::Standard));
    assert!(matches!(protected.audit, AuditMiddleware::Standard));
    assert!(protected.requires_auth());
    assert!(protected.enforces_tenant_guard());
    assert!(protected.enforces_csrf());
    assert!(protected.enforces_policy());
    assert!(protected.performs_audit());

    // optional_auth_chain()
    let optional = optional_auth_chain();
    assert!(matches!(optional.auth, AuthMiddleware::Optional));
    assert!(matches!(optional.tenant_guard, TenantGuardMiddleware::Skip));
    assert!(matches!(optional.csrf, CsrfMiddleware::Skip));
    assert!(matches!(optional.policy, PolicyMiddleware::Skip));
    assert!(matches!(optional.audit, AuditMiddleware::Skip));
    assert!(!optional.requires_auth());

    // api_key_chain()
    let api_key = api_key_chain();
    assert!(matches!(api_key.auth, AuthMiddleware::Dual));
    assert!(matches!(api_key.tenant_guard, TenantGuardMiddleware::Skip));
    assert!(matches!(api_key.csrf, CsrfMiddleware::Skip));
    assert!(matches!(api_key.policy, PolicyMiddleware::Standard));
    assert!(matches!(api_key.audit, AuditMiddleware::Standard));
    assert!(api_key.requires_auth());
    assert!(!api_key.enforces_csrf()); // API keys don't need CSRF

    // internal_chain()
    let internal = internal_chain();
    assert!(matches!(internal.auth, AuthMiddleware::Required));
    assert!(matches!(internal.tenant_guard, TenantGuardMiddleware::Skip));
    assert!(matches!(internal.csrf, CsrfMiddleware::Skip));
    assert!(matches!(internal.policy, PolicyMiddleware::Standard));
    assert!(matches!(internal.audit, AuditMiddleware::Standard));
    assert!(internal.requires_auth());
}

/// Test that chain description is human-readable.
#[test]
fn test_chain_description() {
    let config = protected_chain();
    let desc = config.describe();

    // Should contain all stages in order
    assert!(desc.starts_with("auth(required)"));
    assert!(desc.contains(" -> "));

    // Verify order by checking positions
    let auth_pos = desc.find("auth").unwrap();
    let tenant_pos = desc.find("tenant_guard").unwrap();
    let csrf_pos = desc.find("csrf").unwrap();
    let context_pos = desc.find("context").unwrap();
    let policy_pos = desc.find("policy").unwrap();
    let audit_pos = desc.find("audit").unwrap();

    assert!(auth_pos < tenant_pos);
    assert!(tenant_pos < csrf_pos);
    assert!(csrf_pos < context_pos);
    assert!(context_pos < policy_pos);
    assert!(policy_pos < audit_pos);
}

/// Test that Default impl creates NeedsAuth state.
#[test]
fn test_default_impl() {
    let builder = ProtectedMiddlewareChain::default();

    // Default should be equivalent to new()
    let config = builder
        .with_required_auth()
        .with_tenant_guard()
        .with_csrf()
        .with_context()
        .with_policy()
        .with_audit()
        .build();

    assert!(matches!(config.auth, AuthMiddleware::Required));
}

/// Test that each auth variant produces different configs.
#[test]
fn test_auth_variants() {
    // Required auth
    let required = ProtectedMiddlewareChain::new()
        .with_required_auth()
        .with_tenant_guard()
        .with_csrf()
        .with_context()
        .with_policy()
        .with_audit()
        .build();
    assert!(matches!(required.auth, AuthMiddleware::Required));
    assert!(required.requires_auth());

    // Optional auth
    let optional = ProtectedMiddlewareChain::new()
        .with_optional_auth()
        .with_tenant_guard()
        .with_csrf()
        .with_context()
        .with_policy()
        .with_audit()
        .build();
    assert!(matches!(optional.auth, AuthMiddleware::Optional));
    assert!(!optional.requires_auth());

    // Dual auth
    let dual = ProtectedMiddlewareChain::new()
        .with_dual_auth()
        .with_tenant_guard()
        .with_csrf()
        .with_context()
        .with_policy()
        .with_audit()
        .build();
    assert!(matches!(dual.auth, AuthMiddleware::Dual));
    assert!(dual.requires_auth());
}

// =============================================================================
// Type-State Compile-Time Safety Documentation
// =============================================================================
//
// The following patterns demonstrate what WOULD NOT compile due to the
// type-state enforcement. These are documented as comments because they
// are intentionally invalid code.
//
// PATTERN 1: Skipping Authentication
// ------------------------------------
// The builder starts in NeedsAuth state. Only `with_*_auth()` methods are
// available in this state.
//
// ```compile_fail
// let config = ProtectedMiddlewareChain::new()
//     .with_tenant_guard()  // ERROR: NeedsAuth has no with_tenant_guard method
//     ...
// ```
//
// PATTERN 2: Wrong Order
// ----------------------
// Each state only allows transitioning to the next state.
//
// ```compile_fail
// let config = ProtectedMiddlewareChain::new()
//     .with_required_auth()
//     .with_csrf()  // ERROR: NeedsTenantGuard has no with_csrf method
//     ...
// ```
//
// PATTERN 3: Incomplete Chain
// ---------------------------
// Only the Complete state has the build() method.
//
// ```compile_fail
// let config = ProtectedMiddlewareChain::new()
//     .with_required_auth()
//     .with_tenant_guard()
//     .build()  // ERROR: NeedsCsrf has no build method
// ```
//
// PATTERN 4: State Fabrication
// ----------------------------
// You cannot create a builder in an arbitrary state.
//
// ```compile_fail
// // Cannot construct NeedsPolicy directly
// let builder = ProtectedMiddlewareChain::<NeedsPolicy> { ... };
// ```
//
// These compile-time guarantees prevent middleware ordering bugs that could
// lead to security vulnerabilities like auth bypass or tenant isolation failures.
