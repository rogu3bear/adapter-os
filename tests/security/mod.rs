#![cfg(all(test, feature = "extended-tests"))]
//! Security and Policy Compliance Tests
//!
//! This module contains comprehensive security tests for adapterOS, focusing on:
//!
//! - Policy rule testing and validation
//! - Multi-tenant isolation verification
//! - Evidence validation and integrity
//! - Access control enforcement
//! - Audit trail completeness
//! - Zero-egress policy compliance
//!
//! ## Test Categories
//!
//! - **Policy Tests**: Validate policy rules, gates, and enforcement mechanisms
//! - **Isolation Tests**: Ensure proper tenant separation and resource isolation
//! - **Evidence Tests**: Verify evidence collection, validation, and integrity
//! - **Access Control Tests**: Test authentication, authorization, and permission systems
//! - **Audit Tests**: Validate audit logging, trail completeness, and compliance
//! - **Egress Tests**: Ensure zero-egress policies are enforced
//!
//! ## Security Testing Utilities
//!
//! The `security_test_utils` module provides specialized utilities for security testing,
//! including mock security contexts, policy engines, and compliance validators.

pub mod access_control;
pub mod audit_trail;
pub mod evidence_validation;
pub mod isolation_verification;
pub mod policy_adapter_deny_test;
pub mod policy_rules;
pub mod security_test_utils;
pub mod zero_egress;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn security_test_modules_load() {
        // This test ensures all security test modules can be loaded
        // without compilation errors
    }
}