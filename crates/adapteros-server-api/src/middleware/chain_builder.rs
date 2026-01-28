//! Type-state middleware chain builder for adapterOS
//!
//! Enforces correct middleware ordering at compile time using the type-state pattern.
//! This prevents auth bypass vulnerabilities by ensuring middleware is applied in the
//! correct sequence: Auth -> TenantGuard -> CSRF -> Context -> Policy -> Audit.
//!
//! # Security Rationale
//!
//! Middleware ordering is critical for security. Each layer depends on the previous:
//! - **Auth**: Validates credentials, sets `Claims` and `Principal` in extensions
//! - **TenantGuard**: Requires `Claims` to validate tenant isolation
//! - **CSRF**: Requires auth context to determine if CSRF check is needed
//! - **Context**: Consolidates auth data into `RequestContext`
//! - **Policy**: Requires context for policy evaluation
//! - **Audit**: Requires context for logging
//!
//! # Usage
//!
//! ```ignore
//! use crate::middleware::chain_builder::ProtectedMiddlewareChain;
//!
//! // Correct ordering compiles:
//! let chain = ProtectedMiddlewareChain::new()
//!     .with_required_auth()
//!     .with_tenant_guard()
//!     .with_csrf()
//!     .with_context()
//!     .with_policy()
//!     .with_audit()
//!     .build();
//!
//! // Incorrect ordering fails to compile:
//! // let chain = ProtectedMiddlewareChain::new()
//! //     .with_tenant_guard()  // Error: expected NeedsAuth
//! //     ...
//! ```
//!
//! # Compile-time Enforcement
//!
//! The type-state pattern uses marker types to track the builder's current state.
//! Each `with_*` method is only available when the builder is in the correct state,
//! and transitions to the next state. The `build()` method is only available when
//! all required middleware has been added.
//!
//! This means incorrect ordering is a **compile error**, not a runtime bug.

use std::marker::PhantomData;

// =============================================================================
// Type-State Marker Types
// =============================================================================

/// Marker: Chain needs authentication middleware
pub struct NeedsAuth;

/// Marker: Chain needs tenant guard middleware
pub struct NeedsTenantGuard;

/// Marker: Chain needs CSRF middleware
pub struct NeedsCsrf;

/// Marker: Chain needs context middleware
pub struct NeedsContext;

/// Marker: Chain needs policy enforcement middleware
pub struct NeedsPolicy;

/// Marker: Chain needs audit middleware
pub struct NeedsAudit;

/// Marker: Chain is complete and can be built
pub struct Complete;

// =============================================================================
// Stored Middleware Variants
// =============================================================================

/// Stored middleware configuration for auth layer
#[derive(Clone)]
pub enum AuthMiddleware {
    /// Required auth (rejects unauthenticated requests)
    Required,
    /// Optional auth (allows unauthenticated requests)
    Optional,
    /// Dual auth (API key or JWT)
    Dual,
    /// Custom auth middleware
    Custom,
}

/// Stored middleware configuration for tenant guard
#[derive(Clone)]
pub enum TenantGuardMiddleware {
    /// Standard tenant route guard
    Standard,
    /// Skip tenant guard (for routes that don't need it)
    Skip,
}

/// Stored middleware configuration for CSRF
#[derive(Clone)]
pub enum CsrfMiddleware {
    /// Standard CSRF protection
    Standard,
    /// Skip CSRF (for API-key-only routes)
    Skip,
}

/// Stored middleware configuration for context
#[derive(Clone)]
pub enum ContextMiddleware {
    /// Standard context consolidation
    Standard,
}

/// Stored middleware configuration for policy
#[derive(Clone)]
pub enum PolicyMiddleware {
    /// Standard policy enforcement
    Standard,
    /// Skip policy (for health/status routes)
    Skip,
}

/// Stored middleware configuration for audit
#[derive(Clone)]
pub enum AuditMiddleware {
    /// Standard audit logging
    Standard,
    /// Skip audit (for high-frequency routes)
    Skip,
}

// =============================================================================
// Protected Middleware Chain Builder
// =============================================================================

/// Type-state builder for protected route middleware chains.
///
/// The type parameter `S` tracks the current state of the builder.
/// Methods are only available at the appropriate state, enforcing
/// correct middleware ordering at compile time.
///
/// # Type States
///
/// The builder progresses through states in order:
/// 1. `NeedsAuth` - Initial state, must add auth middleware
/// 2. `NeedsTenantGuard` - After auth, must add tenant guard
/// 3. `NeedsCsrf` - After tenant guard, must add CSRF protection
/// 4. `NeedsContext` - After CSRF, must add context middleware
/// 5. `NeedsPolicy` - After context, must add policy enforcement
/// 6. `NeedsAudit` - After policy, must add audit middleware
/// 7. `Complete` - All middleware added, can call `build()`
pub struct ProtectedMiddlewareChain<S> {
    auth: Option<AuthMiddleware>,
    tenant_guard: Option<TenantGuardMiddleware>,
    csrf: Option<CsrfMiddleware>,
    context: Option<ContextMiddleware>,
    policy: Option<PolicyMiddleware>,
    audit: Option<AuditMiddleware>,
    _state: PhantomData<S>,
}

impl ProtectedMiddlewareChain<NeedsAuth> {
    /// Create a new middleware chain builder.
    ///
    /// The chain starts in the `NeedsAuth` state, requiring authentication
    /// middleware to be added first.
    pub fn new() -> Self {
        Self {
            auth: None,
            tenant_guard: None,
            csrf: None,
            context: None,
            policy: None,
            audit: None,
            _state: PhantomData,
        }
    }

    /// Add required authentication middleware.
    ///
    /// Uses `auth_middleware` which rejects unauthenticated requests.
    pub fn with_required_auth(self) -> ProtectedMiddlewareChain<NeedsTenantGuard> {
        ProtectedMiddlewareChain {
            auth: Some(AuthMiddleware::Required),
            tenant_guard: self.tenant_guard,
            csrf: self.csrf,
            context: self.context,
            policy: self.policy,
            audit: self.audit,
            _state: PhantomData,
        }
    }

    /// Add optional authentication middleware.
    ///
    /// Uses `optional_auth_middleware` which allows unauthenticated requests
    /// but still validates and injects claims if a token is present.
    pub fn with_optional_auth(self) -> ProtectedMiddlewareChain<NeedsTenantGuard> {
        ProtectedMiddlewareChain {
            auth: Some(AuthMiddleware::Optional),
            tenant_guard: self.tenant_guard,
            csrf: self.csrf,
            context: self.context,
            policy: self.policy,
            audit: self.audit,
            _state: PhantomData,
        }
    }

    /// Add dual authentication middleware (API key or JWT).
    ///
    /// Uses `dual_auth_middleware` which accepts either API keys or JWTs.
    pub fn with_dual_auth(self) -> ProtectedMiddlewareChain<NeedsTenantGuard> {
        ProtectedMiddlewareChain {
            auth: Some(AuthMiddleware::Dual),
            tenant_guard: self.tenant_guard,
            csrf: self.csrf,
            context: self.context,
            policy: self.policy,
            audit: self.audit,
            _state: PhantomData,
        }
    }
}

impl Default for ProtectedMiddlewareChain<NeedsAuth> {
    fn default() -> Self {
        Self::new()
    }
}

impl ProtectedMiddlewareChain<NeedsTenantGuard> {
    /// Add standard tenant route guard middleware.
    ///
    /// Enforces tenant isolation for routes with `/tenants/{tenant_id}` in the path.
    pub fn with_tenant_guard(self) -> ProtectedMiddlewareChain<NeedsCsrf> {
        ProtectedMiddlewareChain {
            auth: self.auth,
            tenant_guard: Some(TenantGuardMiddleware::Standard),
            csrf: self.csrf,
            context: self.context,
            policy: self.policy,
            audit: self.audit,
            _state: PhantomData,
        }
    }

    /// Skip tenant guard middleware.
    ///
    /// Use this for routes that don't have tenant IDs in the path.
    pub fn skip_tenant_guard(self) -> ProtectedMiddlewareChain<NeedsCsrf> {
        ProtectedMiddlewareChain {
            auth: self.auth,
            tenant_guard: Some(TenantGuardMiddleware::Skip),
            csrf: self.csrf,
            context: self.context,
            policy: self.policy,
            audit: self.audit,
            _state: PhantomData,
        }
    }
}

impl ProtectedMiddlewareChain<NeedsCsrf> {
    /// Add standard CSRF protection middleware.
    ///
    /// Validates CSRF tokens for cookie-authenticated mutations.
    pub fn with_csrf(self) -> ProtectedMiddlewareChain<NeedsContext> {
        ProtectedMiddlewareChain {
            auth: self.auth,
            tenant_guard: self.tenant_guard,
            csrf: Some(CsrfMiddleware::Standard),
            context: self.context,
            policy: self.policy,
            audit: self.audit,
            _state: PhantomData,
        }
    }

    /// Skip CSRF middleware.
    ///
    /// Use this for API-key-only routes where CSRF protection is not needed.
    pub fn skip_csrf(self) -> ProtectedMiddlewareChain<NeedsContext> {
        ProtectedMiddlewareChain {
            auth: self.auth,
            tenant_guard: self.tenant_guard,
            csrf: Some(CsrfMiddleware::Skip),
            context: self.context,
            policy: self.policy,
            audit: self.audit,
            _state: PhantomData,
        }
    }
}

impl ProtectedMiddlewareChain<NeedsContext> {
    /// Add context consolidation middleware.
    ///
    /// Consolidates auth data into a single `RequestContext` extension.
    pub fn with_context(self) -> ProtectedMiddlewareChain<NeedsPolicy> {
        ProtectedMiddlewareChain {
            auth: self.auth,
            tenant_guard: self.tenant_guard,
            csrf: self.csrf,
            context: Some(ContextMiddleware::Standard),
            policy: self.policy,
            audit: self.audit,
            _state: PhantomData,
        }
    }
}

impl ProtectedMiddlewareChain<NeedsPolicy> {
    /// Add policy enforcement middleware.
    ///
    /// Validates requests against all enabled policy packs.
    pub fn with_policy(self) -> ProtectedMiddlewareChain<NeedsAudit> {
        ProtectedMiddlewareChain {
            auth: self.auth,
            tenant_guard: self.tenant_guard,
            csrf: self.csrf,
            context: self.context,
            policy: Some(PolicyMiddleware::Standard),
            audit: self.audit,
            _state: PhantomData,
        }
    }

    /// Skip policy enforcement middleware.
    ///
    /// Use this for health/status routes that don't need policy checks.
    pub fn skip_policy(self) -> ProtectedMiddlewareChain<NeedsAudit> {
        ProtectedMiddlewareChain {
            auth: self.auth,
            tenant_guard: self.tenant_guard,
            csrf: self.csrf,
            context: self.context,
            policy: Some(PolicyMiddleware::Skip),
            audit: self.audit,
            _state: PhantomData,
        }
    }
}

impl ProtectedMiddlewareChain<NeedsAudit> {
    /// Add audit logging middleware.
    ///
    /// Logs successful and failed operations for audit trails.
    pub fn with_audit(self) -> ProtectedMiddlewareChain<Complete> {
        ProtectedMiddlewareChain {
            auth: self.auth,
            tenant_guard: self.tenant_guard,
            csrf: self.csrf,
            context: self.context,
            policy: self.policy,
            audit: Some(AuditMiddleware::Standard),
            _state: PhantomData,
        }
    }

    /// Skip audit middleware.
    ///
    /// Use this for high-frequency routes where audit logging is not needed.
    pub fn skip_audit(self) -> ProtectedMiddlewareChain<Complete> {
        ProtectedMiddlewareChain {
            auth: self.auth,
            tenant_guard: self.tenant_guard,
            csrf: self.csrf,
            context: self.context,
            policy: self.policy,
            audit: Some(AuditMiddleware::Skip),
            _state: PhantomData,
        }
    }
}

/// The completed middleware chain configuration.
///
/// This type is returned by `build()` and contains all the middleware
/// configuration needed to apply the chain to a router.
#[derive(Clone)]
pub struct MiddlewareChainConfig {
    pub auth: AuthMiddleware,
    pub tenant_guard: TenantGuardMiddleware,
    pub csrf: CsrfMiddleware,
    pub context: ContextMiddleware,
    pub policy: PolicyMiddleware,
    pub audit: AuditMiddleware,
}

impl ProtectedMiddlewareChain<Complete> {
    /// Build the middleware chain configuration.
    ///
    /// This method is only available when all middleware has been configured.
    /// Returns a `MiddlewareChainConfig` that can be used to apply the chain.
    pub fn build(self) -> MiddlewareChainConfig {
        MiddlewareChainConfig {
            auth: self.auth.expect("auth should be set"),
            tenant_guard: self.tenant_guard.expect("tenant_guard should be set"),
            csrf: self.csrf.expect("csrf should be set"),
            context: self.context.expect("context should be set"),
            policy: self.policy.expect("policy should be set"),
            audit: self.audit.expect("audit should be set"),
        }
    }
}

impl MiddlewareChainConfig {
    /// Describes the middleware ordering for documentation purposes.
    ///
    /// Returns a human-readable description of the middleware chain.
    pub fn describe(&self) -> String {
        let mut parts = Vec::new();

        match &self.auth {
            AuthMiddleware::Required => parts.push("auth(required)"),
            AuthMiddleware::Optional => parts.push("auth(optional)"),
            AuthMiddleware::Dual => parts.push("auth(dual)"),
            AuthMiddleware::Custom => parts.push("auth(custom)"),
        }

        match &self.tenant_guard {
            TenantGuardMiddleware::Standard => parts.push("tenant_guard"),
            TenantGuardMiddleware::Skip => parts.push("tenant_guard(skip)"),
        }

        match &self.csrf {
            CsrfMiddleware::Standard => parts.push("csrf"),
            CsrfMiddleware::Skip => parts.push("csrf(skip)"),
        }

        match &self.context {
            ContextMiddleware::Standard => parts.push("context"),
        }

        match &self.policy {
            PolicyMiddleware::Standard => parts.push("policy"),
            PolicyMiddleware::Skip => parts.push("policy(skip)"),
        }

        match &self.audit {
            AuditMiddleware::Standard => parts.push("audit"),
            AuditMiddleware::Skip => parts.push("audit(skip)"),
        }

        parts.join(" -> ")
    }

    /// Returns true if this chain requires authentication.
    pub fn requires_auth(&self) -> bool {
        matches!(self.auth, AuthMiddleware::Required | AuthMiddleware::Dual)
    }

    /// Returns true if this chain enforces tenant isolation.
    pub fn enforces_tenant_guard(&self) -> bool {
        matches!(self.tenant_guard, TenantGuardMiddleware::Standard)
    }

    /// Returns true if this chain enforces CSRF protection.
    pub fn enforces_csrf(&self) -> bool {
        matches!(self.csrf, CsrfMiddleware::Standard)
    }

    /// Returns true if this chain enforces policies.
    pub fn enforces_policy(&self) -> bool {
        matches!(self.policy, PolicyMiddleware::Standard)
    }

    /// Returns true if this chain performs audit logging.
    pub fn performs_audit(&self) -> bool {
        matches!(self.audit, AuditMiddleware::Standard)
    }
}

// =============================================================================
// Convenience Constructors for Common Patterns
// =============================================================================

/// Create a fully protected middleware chain.
///
/// This is the most common configuration for protected API routes:
/// - Required auth
/// - Tenant guard
/// - CSRF protection
/// - Context consolidation
/// - Policy enforcement
/// - Audit logging
pub fn protected_chain() -> MiddlewareChainConfig {
    ProtectedMiddlewareChain::new()
        .with_required_auth()
        .with_tenant_guard()
        .with_csrf()
        .with_context()
        .with_policy()
        .with_audit()
        .build()
}

/// Create an optional-auth middleware chain.
///
/// For routes that work with or without authentication:
/// - Optional auth
/// - Skip tenant guard
/// - Skip CSRF
/// - Context consolidation
/// - Skip policy
/// - Skip audit
pub fn optional_auth_chain() -> MiddlewareChainConfig {
    ProtectedMiddlewareChain::new()
        .with_optional_auth()
        .skip_tenant_guard()
        .skip_csrf()
        .with_context()
        .skip_policy()
        .skip_audit()
        .build()
}

/// Create an API-key-only middleware chain.
///
/// For routes that only accept API key authentication:
/// - Dual auth (API key or JWT)
/// - Skip tenant guard
/// - Skip CSRF (API keys don't need CSRF protection)
/// - Context consolidation
/// - Policy enforcement
/// - Audit logging
pub fn api_key_chain() -> MiddlewareChainConfig {
    ProtectedMiddlewareChain::new()
        .with_dual_auth()
        .skip_tenant_guard()
        .skip_csrf()
        .with_context()
        .with_policy()
        .with_audit()
        .build()
}

/// Create an internal middleware chain.
///
/// For internal routes (worker -> control plane):
/// - Required auth
/// - Skip tenant guard
/// - Skip CSRF
/// - Context consolidation
/// - Policy enforcement
/// - Audit logging
pub fn internal_chain() -> MiddlewareChainConfig {
    ProtectedMiddlewareChain::new()
        .with_required_auth()
        .skip_tenant_guard()
        .skip_csrf()
        .with_context()
        .with_policy()
        .with_audit()
        .build()
}

// =============================================================================
// Tests
// =============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    /// Test that the correct ordering compiles.
    ///
    /// This test verifies that the type-state pattern allows the correct
    /// middleware ordering. If this compiles, the ordering is correct.
    #[test]
    fn test_correct_ordering_compiles() {
        // Full protected chain
        let config = ProtectedMiddlewareChain::new()
            .with_required_auth()
            .with_tenant_guard()
            .with_csrf()
            .with_context()
            .with_policy()
            .with_audit()
            .build();

        assert!(matches!(config.auth, AuthMiddleware::Required));
        assert!(matches!(
            config.tenant_guard,
            TenantGuardMiddleware::Standard
        ));
        assert!(matches!(config.csrf, CsrfMiddleware::Standard));
        assert!(matches!(config.context, ContextMiddleware::Standard));
        assert!(matches!(config.policy, PolicyMiddleware::Standard));
        assert!(matches!(config.audit, AuditMiddleware::Standard));
    }

    /// Test optional auth chain compiles.
    #[test]
    fn test_optional_auth_ordering_compiles() {
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
        assert!(matches!(config.policy, PolicyMiddleware::Skip));
        assert!(matches!(config.audit, AuditMiddleware::Skip));
    }

    /// Test dual auth chain compiles.
    #[test]
    fn test_dual_auth_ordering_compiles() {
        let config = ProtectedMiddlewareChain::new()
            .with_dual_auth()
            .with_tenant_guard()
            .with_csrf()
            .with_context()
            .with_policy()
            .with_audit()
            .build();

        assert!(matches!(config.auth, AuthMiddleware::Dual));
    }

    /// Test convenience constructors.
    #[test]
    fn test_convenience_constructors() {
        let protected = protected_chain();
        assert!(matches!(protected.auth, AuthMiddleware::Required));
        assert!(matches!(
            protected.tenant_guard,
            TenantGuardMiddleware::Standard
        ));
        assert!(matches!(protected.csrf, CsrfMiddleware::Standard));
        assert!(matches!(protected.policy, PolicyMiddleware::Standard));
        assert!(matches!(protected.audit, AuditMiddleware::Standard));

        let optional = optional_auth_chain();
        assert!(matches!(optional.auth, AuthMiddleware::Optional));
        assert!(matches!(optional.tenant_guard, TenantGuardMiddleware::Skip));

        let api_key = api_key_chain();
        assert!(matches!(api_key.auth, AuthMiddleware::Dual));
        assert!(matches!(api_key.csrf, CsrfMiddleware::Skip));

        let internal = internal_chain();
        assert!(matches!(internal.auth, AuthMiddleware::Required));
        assert!(matches!(internal.tenant_guard, TenantGuardMiddleware::Skip));
    }

    // =========================================================================
    // Compile-Time Enforcement Documentation
    // =========================================================================
    //
    // The following code blocks demonstrate INCORRECT orderings that would
    // fail to compile. They are commented out because they are intentionally
    // invalid - the type system prevents these mistakes.
    //
    // ## Example 1: Skipping auth (first step)
    //
    // ```compile_fail
    // // ERROR: no method named `with_tenant_guard` found for struct
    // //        `ProtectedMiddlewareChain<NeedsAuth>`
    // let config = ProtectedMiddlewareChain::new()
    //     .with_tenant_guard()  // <-- Can't skip auth!
    //     .with_csrf()
    //     .with_context()
    //     .with_policy()
    //     .with_audit()
    //     .build();
    // ```
    //
    // ## Example 2: Wrong order (CSRF before tenant guard)
    //
    // ```compile_fail
    // // ERROR: no method named `with_csrf` found for struct
    // //        `ProtectedMiddlewareChain<NeedsTenantGuard>`
    // let config = ProtectedMiddlewareChain::new()
    //     .with_required_auth()
    //     .with_csrf()  // <-- Can't do CSRF before tenant guard!
    //     .with_tenant_guard()
    //     .with_context()
    //     .with_policy()
    //     .with_audit()
    //     .build();
    // ```
    //
    // ## Example 3: Skipping context
    //
    // ```compile_fail
    // // ERROR: no method named `with_policy` found for struct
    // //        `ProtectedMiddlewareChain<NeedsContext>`
    // let config = ProtectedMiddlewareChain::new()
    //     .with_required_auth()
    //     .with_tenant_guard()
    //     .with_csrf()
    //     .with_policy()  // <-- Can't skip context!
    //     .with_audit()
    //     .build();
    // ```
    //
    // ## Example 4: Building incomplete chain
    //
    // ```compile_fail
    // // ERROR: no method named `build` found for struct
    // //        `ProtectedMiddlewareChain<NeedsPolicy>`
    // let config = ProtectedMiddlewareChain::new()
    //     .with_required_auth()
    //     .with_tenant_guard()
    //     .with_csrf()
    //     .with_context()
    //     .build();  // <-- Can't build without policy and audit!
    // ```
    //
    // ## Example 5: Auth bypass attempt
    //
    // ```compile_fail
    // // ERROR: the trait bound `NeedsAuth: ...` is not satisfied
    // let config = ProtectedMiddlewareChain::<NeedsPolicy>::new()  // <-- Can't skip states!
    //     .with_policy()
    //     .with_audit()
    //     .build();
    // ```
    //
    // These compile-time guarantees ensure that middleware ordering bugs
    // are caught during development, not in production.
}
