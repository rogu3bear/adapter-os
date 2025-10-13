//! Category-specific policies for adapter lifecycle management
//!
//! This module defines policies that govern how different adapter categories
//! behave in terms of promotion, demotion, eviction, and memory management.

use crate::state::EvictionPriority;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::time::Duration;

/// Policy configuration for a specific adapter category
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CategoryPolicy {
    /// Minimum time before promotion
    pub promotion_threshold: Duration,
    /// Maximum time before demotion
    pub demotion_threshold: Duration,
    /// Memory limit for this category
    pub memory_limit: usize,
    /// Eviction priority
    pub eviction_priority: EvictionPriority,
    /// Whether to auto-promote based on usage
    pub auto_promote: bool,
    /// Whether to auto-demote based on inactivity
    pub auto_demote: bool,
    /// Maximum number of adapters of this category to keep in memory
    pub max_in_memory: Option<usize>,
    /// Priority boost for routing
    pub routing_priority: f32,
}

impl Default for CategoryPolicy {
    fn default() -> Self {
        Self {
            promotion_threshold: Duration::from_secs(3600), // 1 hour
            demotion_threshold: Duration::from_secs(21600), // 6 hours
            memory_limit: 100 * 1024 * 1024,                // 100MB
            eviction_priority: EvictionPriority::Normal,
            auto_promote: true,
            auto_demote: true,
            max_in_memory: None,
            routing_priority: 1.0,
        }
    }
}

/// Manager for category-specific policies
#[derive(Debug, Clone)]
pub struct CategoryPolicyManager {
    policies: HashMap<String, CategoryPolicy>,
}

impl CategoryPolicyManager {
    /// Create a new policy manager with default policies
    pub fn new() -> Self {
        let mut policies = HashMap::new();

        // Code adapters - high priority, long retention
        policies.insert(
            "code".to_string(),
            CategoryPolicy {
                promotion_threshold: Duration::from_secs(1800), // 30 minutes
                demotion_threshold: Duration::from_secs(86400), // 24 hours
                memory_limit: 200 * 1024 * 1024,                // 200MB
                eviction_priority: EvictionPriority::Low,
                auto_promote: true,
                auto_demote: false,
                max_in_memory: Some(10),
                routing_priority: 1.2,
            },
        );

        // Framework adapters - medium priority
        policies.insert(
            "framework".to_string(),
            CategoryPolicy {
                promotion_threshold: Duration::from_secs(3600), // 1 hour
                demotion_threshold: Duration::from_secs(43200), // 12 hours
                memory_limit: 150 * 1024 * 1024,                // 150MB
                eviction_priority: EvictionPriority::Normal,
                auto_promote: true,
                auto_demote: true,
                max_in_memory: Some(8),
                routing_priority: 1.0,
            },
        );

        // Codebase adapters - tenant-specific, shorter retention
        policies.insert(
            "codebase".to_string(),
            CategoryPolicy {
                promotion_threshold: Duration::from_secs(7200), // 2 hours
                demotion_threshold: Duration::from_secs(14400), // 4 hours
                memory_limit: 300 * 1024 * 1024,                // 300MB
                eviction_priority: EvictionPriority::High,
                auto_promote: false,
                auto_demote: true,
                max_in_memory: Some(5),
                routing_priority: 0.8,
            },
        );

        // Ephemeral adapters - short-lived, immediate eviction
        policies.insert(
            "ephemeral".to_string(),
            CategoryPolicy {
                promotion_threshold: Duration::from_secs(0),
                demotion_threshold: Duration::from_secs(0),
                memory_limit: 50 * 1024 * 1024, // 50MB
                eviction_priority: EvictionPriority::Critical,
                auto_promote: false,
                auto_demote: true,
                max_in_memory: Some(20),
                routing_priority: 0.5,
            },
        );

        Self { policies }
    }

    /// Get policy for a specific category
    pub fn get_policy(&self, category: &str) -> CategoryPolicy {
        self.policies.get(category).cloned().unwrap_or_default()
    }

    /// Update policy for a specific category
    pub fn update_policy(&mut self, category: String, policy: CategoryPolicy) {
        self.policies.insert(category, policy);
    }

    /// Check if an adapter should be promoted based on category policy
    pub fn should_promote(&self, category: &str, time_since_creation: Duration) -> bool {
        let policy = self.get_policy(category);
        policy.auto_promote && time_since_creation >= policy.promotion_threshold
    }

    /// Check if an adapter should be demoted based on category policy
    pub fn should_demote(&self, category: &str, time_since_last_use: Duration) -> bool {
        let policy = self.get_policy(category);
        policy.auto_demote && time_since_last_use >= policy.demotion_threshold
    }

    /// Check if an adapter should be evicted based on category policy
    pub fn should_evict(&self, category: &str, memory_pressure: f32) -> bool {
        let policy = self.get_policy(category);
        let threshold = match policy.eviction_priority {
            EvictionPriority::Never => return false,
            EvictionPriority::Low => 0.9,
            EvictionPriority::Normal => 0.8,
            EvictionPriority::High => 0.7,
            EvictionPriority::Critical => 0.5,
        };
        memory_pressure > threshold
    }

    /// Get memory limit for a category
    pub fn get_memory_limit(&self, category: &str) -> usize {
        self.get_policy(category).memory_limit
    }

    /// Get routing priority for a category
    pub fn get_routing_priority(&self, category: &str) -> f32 {
        self.get_policy(category).routing_priority
    }

    /// Get maximum number of adapters to keep in memory for a category
    pub fn get_max_in_memory(&self, category: &str) -> Option<usize> {
        self.get_policy(category).max_in_memory
    }

    /// Get all categories
    pub fn get_categories(&self) -> Vec<String> {
        self.policies.keys().cloned().collect()
    }

    /// Get policy summary for all categories
    pub fn get_policy_summary(&self) -> HashMap<String, CategoryPolicySummary> {
        let mut summary = HashMap::new();
        for (category, policy) in &self.policies {
            summary.insert(
                category.clone(),
                CategoryPolicySummary {
                    promotion_threshold_ms: policy.promotion_threshold.as_millis() as u64,
                    demotion_threshold_ms: policy.demotion_threshold.as_millis() as u64,
                    memory_limit: policy.memory_limit,
                    eviction_priority: policy.eviction_priority,
                    auto_promote: policy.auto_promote,
                    auto_demote: policy.auto_demote,
                    max_in_memory: policy.max_in_memory,
                    routing_priority: policy.routing_priority,
                },
            );
        }
        summary
    }
}

impl Default for CategoryPolicyManager {
    fn default() -> Self {
        Self::new()
    }
}

/// Summary of category policy for serialization
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CategoryPolicySummary {
    pub promotion_threshold_ms: u64,
    pub demotion_threshold_ms: u64,
    pub memory_limit: usize,
    pub eviction_priority: EvictionPriority,
    pub auto_promote: bool,
    pub auto_demote: bool,
    pub max_in_memory: Option<usize>,
    pub routing_priority: f32,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_category_policies() {
        let manager = CategoryPolicyManager::new();

        // Test code adapter policy
        let code_policy = manager.get_policy("code");
        assert_eq!(code_policy.eviction_priority, EvictionPriority::Low);
        assert!(code_policy.auto_promote);
        assert!(!code_policy.auto_demote);

        // Test ephemeral adapter policy
        let ephemeral_policy = manager.get_policy("ephemeral");
        assert_eq!(
            ephemeral_policy.eviction_priority,
            EvictionPriority::Critical
        );
        assert!(!ephemeral_policy.auto_promote);
        assert!(ephemeral_policy.auto_demote);

        // Test promotion logic
        assert!(manager.should_promote("code", Duration::from_secs(2700))); // 45 minutes
        assert!(!manager.should_promote("code", Duration::from_secs(900))); // 15 minutes

        // Test demotion logic
        assert!(manager.should_demote("framework", Duration::from_secs(54000))); // 15 hours
        assert!(!manager.should_demote("framework", Duration::from_secs(18000))); // 5 hours

        // Test eviction logic
        assert!(manager.should_evict("ephemeral", 0.6));
        assert!(!manager.should_evict("code", 0.6));
    }

    #[test]
    fn test_policy_updates() {
        let mut manager = CategoryPolicyManager::new();

        // Update a policy
        let new_policy = CategoryPolicy {
            promotion_threshold: Duration::from_secs(900), // 15 minutes
            demotion_threshold: Duration::from_secs(3600), // 1 hour
            memory_limit: 50 * 1024 * 1024,
            eviction_priority: EvictionPriority::High,
            auto_promote: false,
            auto_demote: true,
            max_in_memory: Some(3),
            routing_priority: 0.5,
        };

        manager.update_policy("test".to_string(), new_policy);

        let test_policy = manager.get_policy("test");
        assert_eq!(test_policy.eviction_priority, EvictionPriority::High);
        assert!(!test_policy.auto_promote);
        assert!(test_policy.auto_demote);
    }

    #[test]
    fn test_policy_summary() {
        let manager = CategoryPolicyManager::new();
        let summary = manager.get_policy_summary();

        assert!(summary.contains_key("code"));
        assert!(summary.contains_key("framework"));
        assert!(summary.contains_key("codebase"));
        assert!(summary.contains_key("ephemeral"));

        let code_summary = &summary["code"];
        assert_eq!(code_summary.eviction_priority, EvictionPriority::Low);
        assert!(code_summary.auto_promote);
    }
}
