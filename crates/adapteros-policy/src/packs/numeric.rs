//! Numeric Policy Pack
//!
//! Normalizes units internally and validates numeric claims through unit sanity checks.
//! Prevents unit face-plants and fabricated numbers.

use crate::{Audit, Policy, PolicyContext, PolicyId, Severity};
use adapteros_core::{AosError, Result};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// Numeric policy configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NumericConfig {
    /// Canonical units per domain
    pub canonical_units: HashMap<String, String>,
    /// Maximum rounding error allowed
    pub max_rounding_error: f32,
    /// Require units in trace
    pub require_units_in_trace: bool,
    /// Unit conversion rules
    pub unit_conversion: UnitConversion,
    /// Numeric validation rules
    pub validation_rules: ValidationRules,
    /// Precision requirements
    pub precision_requirements: PrecisionRequirements,
}

/// Unit conversion configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UnitConversion {
    /// Enable automatic unit conversion
    pub enable_conversion: bool,
    /// Conversion factors
    pub conversion_factors: HashMap<String, f64>,
    /// Supported units
    pub supported_units: Vec<String>,
}

/// Numeric validation rules
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ValidationRules {
    /// Check for reasonable ranges
    pub check_ranges: bool,
    /// Check for unit consistency
    pub check_unit_consistency: bool,
    /// Check for precision loss
    pub check_precision_loss: bool,
    /// Range limits per unit type
    pub range_limits: HashMap<String, RangeLimit>,
}

/// Range limit for a unit type
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RangeLimit {
    /// Minimum value
    pub min: f64,
    /// Maximum value
    pub max: f64,
    /// Unit of measurement
    pub unit: String,
}

/// Precision requirements
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PrecisionRequirements {
    /// Minimum significant digits
    pub min_significant_digits: usize,
    /// Maximum significant digits
    pub max_significant_digits: usize,
    /// Rounding mode
    pub rounding_mode: RoundingMode,
}

/// Rounding mode
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum RoundingMode {
    /// Round to nearest
    Round,
    /// Truncate
    Truncate,
    /// Ceiling
    Ceiling,
    /// Floor
    Floor,
}

impl Default for NumericConfig {
    fn default() -> Self {
        let mut canonical_units = HashMap::new();
        canonical_units.insert("torque".to_string(), "in_lbf".to_string());
        canonical_units.insert("pressure".to_string(), "psi".to_string());
        canonical_units.insert("temperature".to_string(), "fahrenheit".to_string());
        canonical_units.insert("length".to_string(), "inches".to_string());
        canonical_units.insert("weight".to_string(), "pounds".to_string());

        let mut conversion_factors = HashMap::new();
        conversion_factors.insert("in_lbf_to_nm".to_string(), 0.112984829);
        conversion_factors.insert("nm_to_in_lbf".to_string(), 8.850745793490558);
        conversion_factors.insert("psi_to_pa".to_string(), 6894.75729);
        conversion_factors.insert("fahrenheit_to_celsius".to_string(), 0.555555556);

        let mut range_limits = HashMap::new();
        range_limits.insert(
            "torque".to_string(),
            RangeLimit {
                min: 0.0,
                max: 10000.0,
                unit: "in_lbf".to_string(),
            },
        );
        range_limits.insert(
            "pressure".to_string(),
            RangeLimit {
                min: 0.0,
                max: 10000.0,
                unit: "psi".to_string(),
            },
        );

        Self {
            canonical_units,
            max_rounding_error: 0.5,
            require_units_in_trace: true,
            unit_conversion: UnitConversion {
                enable_conversion: true,
                conversion_factors,
                supported_units: vec![
                    "in_lbf".to_string(),
                    "nm".to_string(),
                    "psi".to_string(),
                    "pa".to_string(),
                    "fahrenheit".to_string(),
                    "celsius".to_string(),
                ],
            },
            validation_rules: ValidationRules {
                check_ranges: true,
                check_unit_consistency: true,
                check_precision_loss: true,
                range_limits,
            },
            precision_requirements: PrecisionRequirements {
                min_significant_digits: 2,
                max_significant_digits: 6,
                rounding_mode: RoundingMode::Round,
            },
        }
    }
}

/// Numeric claim with unit information
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NumericClaim {
    /// The numeric value
    pub value: f64,
    /// Unit of measurement
    pub unit: String,
    /// Context description
    pub context: String,
    /// Precision information
    pub precision: PrecisionInfo,
}

/// Precision information
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PrecisionInfo {
    /// Number of significant digits
    pub significant_digits: usize,
    /// Decimal places
    pub decimal_places: usize,
    /// Rounding error
    pub rounding_error: f64,
}

/// Numeric policy enforcement
pub struct NumericPolicy {
    config: NumericConfig,
}

impl NumericPolicy {
    /// Create a new numeric policy
    pub fn new(config: NumericConfig) -> Self {
        Self { config }
    }

    /// Validate numeric claim
    pub fn validate_numeric_claim(&self, claim: &NumericClaim) -> Result<()> {
        // Check unit consistency
        if self.config.validation_rules.check_unit_consistency {
            self.validate_unit_consistency(claim)?;
        }

        // Check range limits
        if self.config.validation_rules.check_ranges {
            self.validate_range_limits(claim)?;
        }

        // Check precision requirements
        if self.config.validation_rules.check_precision_loss {
            self.validate_precision(claim)?;
        }

        Ok(())
    }

    /// Validate unit consistency
    fn validate_unit_consistency(&self, claim: &NumericClaim) -> Result<()> {
        if !self
            .config
            .unit_conversion
            .supported_units
            .contains(&claim.unit)
        {
            return Err(AosError::PolicyViolation(format!(
                "Unsupported unit: {}",
                claim.unit
            )));
        }

        // Check if unit matches canonical unit for domain
        if let Some(canonical_unit) = self.config.canonical_units.get(&claim.context) {
            if claim.unit != *canonical_unit {
                tracing::warn!(
                    "Unit {} does not match canonical unit {} for context {}",
                    claim.unit,
                    canonical_unit,
                    claim.context
                );
            }
        }

        Ok(())
    }

    /// Validate range limits
    fn validate_range_limits(&self, claim: &NumericClaim) -> Result<()> {
        if let Some(limit) = self
            .config
            .validation_rules
            .range_limits
            .get(&claim.context)
        {
            if claim.value < limit.min || claim.value > limit.max {
                return Err(AosError::PolicyViolation(format!(
                    "Value {} out of range [{}, {}] for {}",
                    claim.value, limit.min, limit.max, claim.context
                )));
            }
        }

        Ok(())
    }

    /// Validate precision requirements
    fn validate_precision(&self, claim: &NumericClaim) -> Result<()> {
        let req = &self.config.precision_requirements;

        if claim.precision.significant_digits < req.min_significant_digits {
            return Err(AosError::PolicyViolation(format!(
                "Insufficient significant digits: {} < {}",
                claim.precision.significant_digits, req.min_significant_digits
            )));
        }

        if claim.precision.significant_digits > req.max_significant_digits {
            return Err(AosError::PolicyViolation(format!(
                "Excessive significant digits: {} > {}",
                claim.precision.significant_digits, req.max_significant_digits
            )));
        }

        if claim.precision.rounding_error > self.config.max_rounding_error as f64 {
            return Err(AosError::PolicyViolation(format!(
                "Rounding error {} exceeds maximum {}",
                claim.precision.rounding_error, self.config.max_rounding_error
            )));
        }

        Ok(())
    }

    /// Convert units
    pub fn convert_units(&self, value: f64, from_unit: &str, to_unit: &str) -> Result<f64> {
        if !self.config.unit_conversion.enable_conversion {
            return Err(AosError::PolicyViolation(
                "Unit conversion is disabled".to_string(),
            ));
        }

        let conversion_key = format!("{}_to_{}", from_unit, to_unit);
        if let Some(factor) = self
            .config
            .unit_conversion
            .conversion_factors
            .get(&conversion_key)
        {
            Ok(value * factor)
        } else {
            Err(AosError::PolicyViolation(format!(
                "No conversion factor found for {} to {}",
                from_unit, to_unit
            )))
        }
    }

    /// Normalize units to canonical form
    pub fn normalize_units(&self, claim: &mut NumericClaim) -> Result<()> {
        if let Some(canonical_unit) = self.config.canonical_units.get(&claim.context) {
            if claim.unit != *canonical_unit {
                let converted_value =
                    self.convert_units(claim.value, &claim.unit, canonical_unit)?;
                claim.value = converted_value;
                claim.unit = canonical_unit.clone();
            }
        }

        Ok(())
    }

    /// Calculate precision information
    pub fn calculate_precision(&self, value: f64) -> PrecisionInfo {
        let value_str = format!("{}", value);
        let significant_digits = value_str.chars().filter(|c| c.is_ascii_digit()).count();

        let decimal_places = if value_str.contains('.') {
            value_str.split('.').nth(1).unwrap_or("").len()
        } else {
            0
        };

        let rounding_error = (value - value.round()).abs();

        PrecisionInfo {
            significant_digits,
            decimal_places,
            rounding_error,
        }
    }

    /// Validate units in trace
    pub fn validate_units_in_trace(&self, trace: &str) -> Result<()> {
        if self.config.require_units_in_trace {
            // Check if trace contains unit information
            let has_units = self
                .config
                .canonical_units
                .values()
                .any(|unit| trace.contains(unit));
            if !has_units {
                return Err(AosError::PolicyViolation(
                    "Trace must contain unit information".to_string(),
                ));
            }
        }

        Ok(())
    }
}

impl Policy for NumericPolicy {
    fn id(&self) -> PolicyId {
        PolicyId::Numeric
    }

    fn name(&self) -> &'static str {
        "Numeric"
    }

    fn severity(&self) -> Severity {
        Severity::Medium
    }

    fn enforce(&self, _ctx: &dyn PolicyContext) -> Result<Audit> {
        let violations = Vec::new();

        // Basic validation - in a real implementation, this would check
        // specific policy requirements

        if violations.is_empty() {
            Ok(Audit::passed(self.id()))
        } else {
            Ok(Audit::failed(self.id(), violations))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_numeric_policy_creation() {
        let config = NumericConfig::default();
        let policy = NumericPolicy::new(config);
        assert_eq!(policy.id(), PolicyId::Numeric);
        assert_eq!(policy.name(), "Numeric");
        assert_eq!(policy.severity(), Severity::Medium);
    }

    #[test]
    fn test_numeric_config_default() {
        let config = NumericConfig::default();
        assert_eq!(config.max_rounding_error, 0.5);
        assert!(config.require_units_in_trace);
        assert!(config.unit_conversion.enable_conversion);
    }

    #[test]
    fn test_validate_numeric_claim() {
        let config = NumericConfig::default();
        let policy = NumericPolicy::new(config);

        let valid_claim = NumericClaim {
            value: 100.0,
            unit: "in_lbf".to_string(),
            context: "torque".to_string(),
            precision: PrecisionInfo {
                significant_digits: 3,
                decimal_places: 1,
                rounding_error: 0.1,
            },
        };

        assert!(policy.validate_numeric_claim(&valid_claim).is_ok());
    }

    #[test]
    fn test_unit_consistency_validation() {
        let config = NumericConfig::default();
        let policy = NumericPolicy::new(config);

        let invalid_unit_claim = NumericClaim {
            value: 100.0,
            unit: "invalid_unit".to_string(),
            context: "torque".to_string(),
            precision: PrecisionInfo {
                significant_digits: 3,
                decimal_places: 1,
                rounding_error: 0.1,
            },
        };

        assert!(policy.validate_numeric_claim(&invalid_unit_claim).is_err());
    }

    #[test]
    fn test_range_limits_validation() {
        let config = NumericConfig::default();
        let policy = NumericPolicy::new(config);

        let out_of_range_claim = NumericClaim {
            value: 20000.0, // Above max limit
            unit: "in_lbf".to_string(),
            context: "torque".to_string(),
            precision: PrecisionInfo {
                significant_digits: 3,
                decimal_places: 1,
                rounding_error: 0.1,
            },
        };

        assert!(policy.validate_numeric_claim(&out_of_range_claim).is_err());
    }

    #[test]
    fn test_precision_validation() {
        let config = NumericConfig::default();
        let policy = NumericPolicy::new(config);

        let insufficient_precision_claim = NumericClaim {
            value: 100.0,
            unit: "in_lbf".to_string(),
            context: "torque".to_string(),
            precision: PrecisionInfo {
                significant_digits: 1, // Below minimum
                decimal_places: 1,
                rounding_error: 0.1,
            },
        };

        assert!(policy
            .validate_numeric_claim(&insufficient_precision_claim)
            .is_err());
    }

    #[test]
    fn test_unit_conversion() {
        let config = NumericConfig::default();
        let policy = NumericPolicy::new(config);

        let result = policy.convert_units(100.0, "in_lbf", "nm");
        assert!(result.is_ok());
        assert!(result.unwrap() > 0.0);
    }

    #[test]
    fn test_normalize_units() {
        let config = NumericConfig::default();
        let policy = NumericPolicy::new(config);

        let mut claim = NumericClaim {
            value: 100.0,
            unit: "nm".to_string(),
            context: "torque".to_string(),
            precision: PrecisionInfo {
                significant_digits: 3,
                decimal_places: 1,
                rounding_error: 0.1,
            },
        };

        assert!(policy.normalize_units(&mut claim).is_ok());
        assert_eq!(claim.unit, "in_lbf"); // Should be converted to canonical unit
    }

    #[test]
    fn test_calculate_precision() {
        let config = NumericConfig::default();
        let policy = NumericPolicy::new(config);

        let precision = policy.calculate_precision(123.456);
        assert_eq!(precision.significant_digits, 6);
        assert_eq!(precision.decimal_places, 3);
    }

    #[test]
    fn test_validate_units_in_trace() {
        let config = NumericConfig::default();
        let policy = NumericPolicy::new(config);

        // Valid trace with units
        assert!(policy.validate_units_in_trace("torque: 100 in_lbf").is_ok());

        // Invalid trace without units
        assert!(policy.validate_units_in_trace("torque: 100").is_err());
    }
}
