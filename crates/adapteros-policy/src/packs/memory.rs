//! Memory Policy Pack
//!
//! Enforces memory management policies including headroom requirements,
//! eviction order, and K reduction strategies.

use crate::{Audit, Policy, PolicyContext, PolicyId, Severity, Violation};
use adapteros_core::{AosError, Result};
use serde::{Deserialize, Serialize};

/// Memory policy configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MemoryConfig {
    /// Minimum headroom percentage
    pub min_headroom_pct: f64,
    /// Eviction order for adapters
    pub evict_order: Vec<EvictionOrder>,
    /// Whether to reduce K before evicting hot adapters
    pub k_reduce_before_evict: bool,
    /// Maximum memory usage percentage
    pub max_memory_usage_pct: f64,
    /// Memory pressure threshold
    pub memory_pressure_threshold: f64,
}

/// Eviction order for adapters
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum EvictionOrder {
    /// Evict ephemeral adapters first (TTL expired)
    EphemeralTtl,
    /// Evict cold adapters (least recently used)
    ColdLru,
    /// Evict warm adapters (moderately used)
    WarmLru,
    /// Evict hot adapters (frequently used)
    HotLru,
}

impl Default for MemoryConfig {
    fn default() -> Self {
        Self {
            min_headroom_pct: 15.0,
            evict_order: vec![
                EvictionOrder::EphemeralTtl,
                EvictionOrder::ColdLru,
                EvictionOrder::WarmLru,
            ],
            k_reduce_before_evict: true,
            max_memory_usage_pct: 85.0,
            memory_pressure_threshold: 80.0,
        }
    }
}

/// Memory usage statistics
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MemoryStats {
    pub total_memory_mb: u64,
    pub used_memory_mb: u64,
    pub available_memory_mb: u64,
    pub headroom_mb: u64,
    pub headroom_pct: f64,
    pub memory_pressure: MemoryPressureLevel,
    pub timestamp: u64,
}

/// Memory pressure levels
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum MemoryPressureLevel {
    /// Low pressure - normal operation
    Low,
    /// Medium pressure - monitoring
    Medium,
    /// High pressure - eviction needed
    High,
    /// Critical pressure - immediate action required
    Critical,
}

/// Adapter memory usage information
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AdapterMemoryInfo {
    pub adapter_id: String,
    pub memory_usage_mb: u64,
    pub adapter_type: AdapterType,
    pub last_accessed: u64,
    pub access_count: u64,
    pub ttl_expires: Option<u64>,
    pub priority: AdapterPriority,
}

/// Types of adapters for memory management
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum AdapterType {
    /// Ephemeral adapter (TTL-bound)
    Ephemeral,
    /// Directory-specific adapter
    DirectorySpecific,
    /// Framework adapter
    Framework,
    /// Code adapter
    Code,
    /// Base adapter
    Base,
}

/// Adapter priority levels
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum AdapterPriority {
    /// Low priority (can be evicted easily)
    Low,
    /// Medium priority
    Medium,
    /// High priority (should be retained)
    High,
    /// Critical priority (never evict)
    Critical,
}

/// Memory eviction decision
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EvictionDecision {
    pub adapters_to_evict: Vec<AdapterMemoryInfo>,
    pub k_reduction: Option<u32>,
    pub estimated_memory_freed_mb: u64,
    pub eviction_reason: EvictionReason,
    pub timestamp: u64,
}

/// Reasons for eviction
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum EvictionReason {
    /// Memory pressure exceeded threshold
    MemoryPressure,
    /// Headroom below minimum
    HeadroomBelowMinimum,
    /// TTL expired
    TtlExpired,
    /// Manual eviction
    Manual,
    /// System shutdown
    SystemShutdown,
}

/// Memory policy implementation
pub struct MemoryPolicy {
    config: MemoryConfig,
}

impl MemoryPolicy {
    /// Create new memory policy
    pub fn new(config: MemoryConfig) -> Self {
        Self { config }
    }

    /// Calculate memory statistics
    pub fn calculate_stats(&self, total_memory_mb: u64, used_memory_mb: u64) -> MemoryStats {
        let available_memory_mb = total_memory_mb.saturating_sub(used_memory_mb);
        let headroom_mb = available_memory_mb;
        let headroom_pct = if total_memory_mb > 0 {
            (headroom_mb as f64 / total_memory_mb as f64) * 100.0
        } else {
            0.0
        };

        let memory_pressure = if headroom_pct < self.config.memory_pressure_threshold {
            if headroom_pct < self.config.min_headroom_pct {
                MemoryPressureLevel::Critical
            } else {
                MemoryPressureLevel::High
            }
        } else if headroom_pct < self.config.min_headroom_pct + 5.0 {
            MemoryPressureLevel::Medium
        } else {
            MemoryPressureLevel::Low
        };

        MemoryStats {
            total_memory_mb,
            used_memory_mb,
            available_memory_mb,
            headroom_mb,
            headroom_pct,
            memory_pressure,
            timestamp: std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap_or_default()
                .as_millis() as u64,
        }
    }

    /// Determine eviction strategy
    pub fn determine_eviction(
        &self,
        adapters: &[AdapterMemoryInfo],
        current_k: u32,
    ) -> Result<EvictionDecision> {
        let mut adapters_to_evict = Vec::new();
        let mut k_reduction = None;
        let mut estimated_memory_freed = 0u64;

        // Sort adapters by eviction order
        let mut sorted_adapters = adapters.to_vec();
        sorted_adapters.sort_by(|a, b| {
            let a_order = self.get_eviction_order(&a.adapter_type);
            let b_order = self.get_eviction_order(&b.adapter_type);
            a_order.cmp(&b_order)
        });

        // First, try to evict ephemeral adapters
        for adapter in &sorted_adapters {
            if matches!(adapter.adapter_type, AdapterType::Ephemeral) {
                if let Some(ttl) = adapter.ttl_expires {
                    let now = std::time::SystemTime::now()
                        .duration_since(std::time::UNIX_EPOCH)
                        .unwrap_or_default()
                        .as_millis() as u64;
                    if now > ttl {
                        adapters_to_evict.push(adapter.clone());
                        estimated_memory_freed += adapter.memory_usage_mb;
                    }
                }
            }
        }

        // If still not enough memory freed, reduce K
        if self.config.k_reduce_before_evict && current_k > 1 {
            k_reduction = Some(current_k - 1);
        }

        // If still not enough, evict cold adapters
        if estimated_memory_freed < 100 {
            // Less than 100MB freed
            for adapter in &sorted_adapters {
                if (matches!(adapter.adapter_type, AdapterType::DirectorySpecific)
                    || matches!(adapter.adapter_type, AdapterType::Framework))
                    && matches!(adapter.priority, AdapterPriority::Low)
                {
                    adapters_to_evict.push(adapter.clone());
                    estimated_memory_freed += adapter.memory_usage_mb;
                }
            }
        }

        // If still not enough, evict warm adapters
        if estimated_memory_freed < 200 {
            // Less than 200MB freed
            for adapter in &sorted_adapters {
                if (matches!(adapter.adapter_type, AdapterType::DirectorySpecific)
                    || matches!(adapter.adapter_type, AdapterType::Framework))
                    && matches!(adapter.priority, AdapterPriority::Medium)
                {
                    adapters_to_evict.push(adapter.clone());
                    estimated_memory_freed += adapter.memory_usage_mb;
                }
            }
        }

        Ok(EvictionDecision {
            adapters_to_evict,
            k_reduction,
            estimated_memory_freed_mb: estimated_memory_freed,
            eviction_reason: EvictionReason::MemoryPressure,
            timestamp: std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap_or_default()
                .as_millis() as u64,
        })
    }

    /// Get eviction order for adapter type
    fn get_eviction_order(&self, adapter_type: &AdapterType) -> usize {
        match adapter_type {
            AdapterType::Ephemeral => 0,
            AdapterType::DirectorySpecific => 1,
            AdapterType::Framework => 2,
            AdapterType::Code => 3,
            AdapterType::Base => 4,
        }
    }

    /// Validate memory configuration
    pub fn validate_config(&self) -> Result<()> {
        if self.config.min_headroom_pct < 0.0 || self.config.min_headroom_pct > 100.0 {
            return Err(AosError::PolicyViolation(
                "min_headroom_pct must be between 0 and 100".to_string(),
            ));
        }

        if self.config.max_memory_usage_pct < 0.0 || self.config.max_memory_usage_pct > 100.0 {
            return Err(AosError::PolicyViolation(
                "max_memory_usage_pct must be between 0 and 100".to_string(),
            ));
        }

        if self.config.memory_pressure_threshold < 0.0
            || self.config.memory_pressure_threshold > 100.0
        {
            return Err(AosError::PolicyViolation(
                "memory_pressure_threshold must be between 0 and 100".to_string(),
            ));
        }

        if self.config.min_headroom_pct >= self.config.max_memory_usage_pct {
            return Err(AosError::PolicyViolation(
                "min_headroom_pct must be less than max_memory_usage_pct".to_string(),
            ));
        }

        Ok(())
    }
}

/// Context for memory policy enforcement
#[derive(Debug)]
pub struct MemoryContext {
    pub total_memory_mb: u64,
    pub used_memory_mb: u64,
    pub adapters: Vec<AdapterMemoryInfo>,
    pub current_k: u32,
    pub tenant_id: String,
}

impl PolicyContext for MemoryContext {
    fn context_type(&self) -> &str {
        "memory"
    }

    fn as_any(&self) -> &dyn std::any::Any {
        self
    }
}

impl Policy for MemoryPolicy {
    fn id(&self) -> PolicyId {
        PolicyId::Memory
    }

    fn name(&self) -> &'static str {
        "Memory"
    }

    fn severity(&self) -> Severity {
        Severity::High
    }

    fn enforce(&self, ctx: &dyn PolicyContext) -> Result<Audit> {
        let memory_ctx = ctx
            .as_any()
            .downcast_ref::<MemoryContext>()
            .ok_or_else(|| AosError::PolicyViolation("Invalid memory context".to_string()))?;

        // Validate configuration
        self.validate_config()?;

        // Calculate memory statistics
        let stats = self.calculate_stats(memory_ctx.total_memory_mb, memory_ctx.used_memory_mb);

        let mut violations = Vec::new();
        let mut warnings = Vec::new();

        // Check headroom requirements
        if stats.headroom_pct < self.config.min_headroom_pct {
            violations.push(Violation {
                severity: Severity::High,
                message: "Memory headroom below minimum".to_string(),
                details: Some(format!(
                    "Headroom: {:.2}%, Minimum: {:.2}%",
                    stats.headroom_pct, self.config.min_headroom_pct
                )),
            });
        }

        // Check memory usage
        let usage_pct =
            (memory_ctx.used_memory_mb as f64 / memory_ctx.total_memory_mb as f64) * 100.0;
        if usage_pct > self.config.max_memory_usage_pct {
            violations.push(Violation {
                severity: Severity::Critical,
                message: "Memory usage exceeded maximum".to_string(),
                details: Some(format!(
                    "Usage: {:.2}%, Maximum: {:.2}%",
                    usage_pct, self.config.max_memory_usage_pct
                )),
            });
        }

        // Check memory pressure
        match stats.memory_pressure {
            MemoryPressureLevel::High => {
                warnings.push("High memory pressure detected".to_string());
            }
            MemoryPressureLevel::Critical => {
                violations.push(Violation {
                    severity: Severity::Critical,
                    message: "Critical memory pressure".to_string(),
                    details: Some(format!("Headroom: {:.2}%", stats.headroom_pct)),
                });
            }
            _ => {}
        }

        // Determine eviction strategy if needed
        if stats.memory_pressure == MemoryPressureLevel::High
            || stats.memory_pressure == MemoryPressureLevel::Critical
        {
            let eviction_decision =
                self.determine_eviction(&memory_ctx.adapters, memory_ctx.current_k)?;

            if !eviction_decision.adapters_to_evict.is_empty() {
                warnings.push(format!(
                    "Eviction recommended: {} adapters, {}MB freed",
                    eviction_decision.adapters_to_evict.len(),
                    eviction_decision.estimated_memory_freed_mb
                ));
            }

            if let Some(k_reduction) = eviction_decision.k_reduction {
                warnings.push(format!(
                    "K reduction recommended: {} -> {}",
                    memory_ctx.current_k, k_reduction
                ));
            }
        }

        Ok(Audit {
            policy_id: PolicyId::Memory,
            passed: violations.is_empty(),
            violations,
            warnings,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_memory_config_default() {
        let config = MemoryConfig::default();
        assert_eq!(config.min_headroom_pct, 15.0);
        assert_eq!(config.max_memory_usage_pct, 85.0);
        assert!(config.k_reduce_before_evict);
    }

    #[test]
    fn test_memory_policy_creation() {
        let config = MemoryConfig::default();
        let policy = MemoryPolicy::new(config);
        assert_eq!(policy.id(), PolicyId::Memory);
    }

    #[test]
    fn test_memory_stats_calculation() {
        let config = MemoryConfig::default();
        let policy = MemoryPolicy::new(config);

        let stats = policy.calculate_stats(1000, 800); // 1000MB total, 800MB used
        assert_eq!(stats.total_memory_mb, 1000);
        assert_eq!(stats.used_memory_mb, 800);
        assert_eq!(stats.available_memory_mb, 200);
        assert_eq!(stats.headroom_pct, 20.0);
    }

    #[test]
    fn test_memory_pressure_detection() {
        let config = MemoryConfig::default();
        let policy = MemoryPolicy::new(config);

        // Low pressure (high headroom > 80%)
        let stats = policy.calculate_stats(1000, 100);
        assert!(matches!(stats.memory_pressure, MemoryPressureLevel::Low));

        // High pressure (headroom between 15% and 80%)
        let stats = policy.calculate_stats(1000, 850);
        assert!(matches!(stats.memory_pressure, MemoryPressureLevel::High));

        // Critical pressure (headroom < 15%)
        let stats = policy.calculate_stats(1000, 900);
        assert!(matches!(
            stats.memory_pressure,
            MemoryPressureLevel::Critical
        ));
    }

    #[test]
    fn test_eviction_decision() {
        let config = MemoryConfig::default();
        let policy = MemoryPolicy::new(config);

        let adapters = vec![
            AdapterMemoryInfo {
                adapter_id: "adapter1".to_string(),
                memory_usage_mb: 100,
                adapter_type: AdapterType::Ephemeral,
                last_accessed: 1000,
                access_count: 10,
                ttl_expires: Some(500), // Expired
                priority: AdapterPriority::Low,
            },
            AdapterMemoryInfo {
                adapter_id: "adapter2".to_string(),
                memory_usage_mb: 200,
                adapter_type: AdapterType::DirectorySpecific,
                last_accessed: 2000,
                access_count: 50,
                ttl_expires: None,
                priority: AdapterPriority::Medium,
            },
        ];

        let decision = policy.determine_eviction(&adapters, 3).unwrap();
        assert!(!decision.adapters_to_evict.is_empty());
        assert!(decision.estimated_memory_freed_mb > 0);
    }

    #[test]
    fn test_memory_config_validation() {
        let mut config = MemoryConfig::default();
        config.min_headroom_pct = 150.0; // Invalid
        let policy = MemoryPolicy::new(config);

        assert!(policy.validate_config().is_err());
    }
}
