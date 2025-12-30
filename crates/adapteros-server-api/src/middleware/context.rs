//! Request context middleware for AdapterOS API
//!
//! Consolidates request-scoped data (claims, request ID, client IP) into a single
//! `RequestContext` type, with convenient extractors for handlers.
//!
//! # Usage
//!
//! ```ignore
//! use crate::middleware::context::{Ctx, AuthCtx, RequestContext};
//!
//! // Full context (may have claims or not)
//! pub async fn my_handler(ctx: Ctx) -> ApiResult<Response> {
//!     let request_id = &ctx.request_id;
//!     if let Some(claims) = &ctx.claims {
//!         // authenticated request
//!     }
//! }
//!
//! // Authenticated context (always has claims, rejects if not authenticated)
//! pub async fn protected_handler(auth: AuthCtx) -> ApiResult<Response> {
//!     let user_id = &auth.claims.sub;
//! }
//! ```

use crate::api_error::ApiError;
use crate::auth::{AuthMode, Claims, Principal};
use crate::ip_extraction::ClientIp;
use crate::request_id::RequestId;
use axum::{
    body::Body,
    extract::{FromRequestParts, Request},
    http::request::Parts,
    middleware::Next,
    response::Response,
};
use std::sync::Arc;

/// Consolidated request context containing all request-scoped data
#[derive(Debug, Clone)]
pub struct RequestContext {
    /// JWT claims (if authenticated)
    pub claims: Option<Claims>,
    /// Normalized principal (if authenticated)
    pub principal: Option<Principal>,
    /// How this request was authenticated (or not)
    pub auth_mode: AuthMode,
    /// Unique request identifier for tracing
    pub request_id: String,
    /// Client IP address
    pub client_ip: String,
}

impl RequestContext {
    /// Get the user ID from claims, or "anonymous" if not authenticated
    pub fn user_id(&self) -> &str {
        if let Some(principal) = &self.principal {
            principal.principal_id.as_str()
        } else {
            self.claims
                .as_ref()
                .map(|c| c.sub.as_str())
                .unwrap_or("anonymous")
        }
    }

    /// Get the tenant ID from claims, or "system" if not authenticated
    pub fn tenant_id(&self) -> &str {
        if let Some(principal) = &self.principal {
            principal.tenant_id.as_str()
        } else {
            self.claims
                .as_ref()
                .map(|c| c.tenant_id.as_str())
                .unwrap_or("system")
        }
    }

    /// Check if the request is authenticated
    pub fn is_authenticated(&self) -> bool {
        match &self.principal {
            Some(principal) => principal.auth_mode.is_authenticated(),
            None => self.claims.is_some(),
        }
    }

    /// Get principal if present
    pub fn principal(&self) -> Option<&Principal> {
        self.principal.as_ref()
    }

    /// Effective auth mode (defaults to unauthenticated)
    pub fn auth_mode(&self) -> AuthMode {
        self.principal
            .as_ref()
            .map(|p| p.auth_mode.clone())
            .unwrap_or_else(|| self.auth_mode.clone())
    }
}

/// Middleware that consolidates request context into a single extension
///
/// This middleware should run after auth_middleware so that Claims are available.
/// It combines Claims, RequestId, and ClientIp into a single RequestContext.
pub async fn context_middleware(req: Request<Body>, next: Next) -> Response {
    let (mut parts, body) = req.into_parts();

    // Extract existing extensions
    let claims = parts.extensions.get::<Claims>().cloned();
    let principal = parts.extensions.get::<Principal>().cloned();
    let auth_mode = parts
        .extensions
        .get::<AuthMode>()
        .cloned()
        .or_else(|| principal.as_ref().map(|p| p.auth_mode.clone()))
        .unwrap_or(AuthMode::Unauthenticated);
    let request_id = parts
        .extensions
        .get::<RequestId>()
        .map(|r| r.0.clone())
        .unwrap_or_else(|| uuid::Uuid::new_v4().to_string());
    let client_ip = parts
        .extensions
        .get::<ClientIp>()
        .map(|c| c.0.clone())
        .unwrap_or_else(|| "unknown".to_string());

    // Create consolidated context
    let ctx = Arc::new(RequestContext {
        claims,
        principal,
        auth_mode,
        request_id,
        client_ip,
    });

    // Insert context into extensions
    parts.extensions.insert(ctx.clone());

    // Reconstruct request and continue
    let req = Request::from_parts(parts, body);
    let mut response =
        adapteros_db::adapters::with_tenant_scope(|| async move { next.run(req).await }).await;
    // Attach context to the response so outer middleware can log tenant/user IDs.
    response.extensions_mut().insert(ctx);
    response
}

/// Extractor for full request context (may or may not be authenticated)
///
/// Use this when you need request metadata but authentication is optional.
#[derive(Debug, Clone)]
pub struct Ctx(pub Arc<RequestContext>);

impl std::ops::Deref for Ctx {
    type Target = RequestContext;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl<S> FromRequestParts<S> for Ctx
where
    S: Send + Sync,
{
    type Rejection = ApiError;

    async fn from_request_parts(parts: &mut Parts, _state: &S) -> Result<Self, Self::Rejection> {
        parts
            .extensions
            .get::<Arc<RequestContext>>()
            .cloned()
            .map(Ctx)
            .ok_or_else(|| {
                ApiError::internal("RequestContext not found - is context_middleware configured?")
            })
    }
}

/// Extractor for authenticated request context (requires valid claims)
///
/// Use this for protected handlers - extraction fails if not authenticated.
#[derive(Debug, Clone)]
pub struct AuthCtx {
    /// The authenticated user's claims
    pub claims: Claims,
    /// Normalized principal derived from claims/token
    pub principal: Principal,
    /// How the request was authenticated
    pub auth_mode: AuthMode,
    /// Unique request identifier
    pub request_id: String,
    /// Client IP address
    pub client_ip: String,
}

impl AuthCtx {
    /// Get the user ID
    pub fn user_id(&self) -> &str {
        &self.principal.principal_id
    }

    /// Get the tenant ID
    pub fn tenant_id(&self) -> &str {
        &self.principal.tenant_id
    }

    /// Get the user's primary role
    pub fn role(&self) -> &str {
        &self.claims.role
    }

    /// Check if user has a specific role
    pub fn has_role(&self, role: &str) -> bool {
        self.claims.role == role || self.claims.roles.iter().any(|r| r == role)
    }

    pub fn principal(&self) -> &Principal {
        &self.principal
    }

    pub fn auth_mode(&self) -> &AuthMode {
        &self.auth_mode
    }
}

impl<S> FromRequestParts<S> for AuthCtx
where
    S: Send + Sync,
{
    type Rejection = ApiError;

    async fn from_request_parts(parts: &mut Parts, _state: &S) -> Result<Self, Self::Rejection> {
        let ctx = parts
            .extensions
            .get::<Arc<RequestContext>>()
            .ok_or_else(|| {
                ApiError::internal("RequestContext not found - is context_middleware configured?")
            })?;

        let claims = ctx
            .claims
            .clone()
            .ok_or_else(|| ApiError::unauthorized("authentication required"))?;

        let principal = ctx
            .principal
            .clone()
            .ok_or_else(|| ApiError::unauthorized("authentication required"))?;

        Ok(AuthCtx {
            claims,
            principal,
            auth_mode: ctx.auth_mode.clone(),
            request_id: ctx.request_id.clone(),
            client_ip: ctx.client_ip.clone(),
        })
    }
}

/// Extractor for authenticated principal only (claims are retained for compatibility)
#[derive(Debug, Clone)]
pub struct PrincipalCtx(pub Principal);

impl std::ops::Deref for PrincipalCtx {
    type Target = Principal;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl<S> FromRequestParts<S> for PrincipalCtx
where
    S: Send + Sync,
{
    type Rejection = ApiError;

    async fn from_request_parts(parts: &mut Parts, _state: &S) -> Result<Self, Self::Rejection> {
        let ctx = parts
            .extensions
            .get::<Arc<RequestContext>>()
            .ok_or_else(|| {
                ApiError::internal("RequestContext not found - is context_middleware configured?")
            })?;

        let principal = ctx
            .principal
            .clone()
            .ok_or_else(|| ApiError::unauthorized("authentication required"))?;

        Ok(PrincipalCtx(principal))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_request_context_defaults() {
        let ctx = RequestContext {
            claims: None,
            principal: None,
            auth_mode: AuthMode::Unauthenticated,
            request_id: "test-123".to_string(),
            client_ip: "127.0.0.1".to_string(),
        };
        assert_eq!(ctx.user_id(), "anonymous");
        assert_eq!(ctx.tenant_id(), "system");
        assert!(!ctx.is_authenticated());
    }
}
