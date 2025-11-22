//! API response schema validation
//!
//! Provides comprehensive validation of API responses against predefined schemas
//! to ensure consistency and correctness.

use adapteros_core::{AosError, Result};
use adapteros_telemetry::events::schema_validation::*;
use adapteros_telemetry::TelemetryWriter;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::collections::HashMap;
use std::sync::Arc;

/// Response schema definition
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ResponseSchema {
    /// Schema name for identification
    pub name: String,
    /// JSON Schema definition
    pub schema: Value,
    /// Whether this schema is required for validation
    pub required: bool,
    /// Schema version for compatibility tracking
    pub version: String,
}

/// Schema validation result
#[derive(Debug, Clone)]
pub struct ValidationResult {
    /// Whether validation passed
    pub valid: bool,
    /// Validation errors if any
    pub errors: Vec<String>,
    /// Schema name that was validated against
    pub schema_name: String,
    /// Response size in bytes
    pub response_size: usize,
    /// Validation duration in microseconds
    pub validation_time_us: u64,
}

/// Response schema validator
pub struct ResponseSchemaValidator {
    schemas: HashMap<String, ResponseSchema>,
    telemetry: Option<TelemetryWriter>,
}

impl ResponseSchemaValidator {
    /// Create a new validator with default schemas
    pub fn new(telemetry: Option<TelemetryWriter>) -> Self {
        let mut validator = Self {
            schemas: HashMap::new(),
            telemetry,
        };

        // Initialize with common response schemas
        validator.register_default_schemas();
        validator
    }

    /// Register a response schema
    pub fn register_schema(&mut self, schema: ResponseSchema) -> Result<()> {
        if self.schemas.contains_key(&schema.name) {
            return Err(AosError::Validation(format!(
                "Schema '{}' already registered",
                schema.name
            )));
        }

        self.schemas.insert(schema.name.clone(), schema);
        Ok(())
    }

    /// Validate a response against a named schema
    pub async fn validate_response(
        &self,
        response: &Value,
        schema_name: &str,
    ) -> Result<ValidationResult> {
        let start_time = std::time::Instant::now();

        let schema = self
            .schemas
            .get(schema_name)
            .ok_or_else(|| AosError::Validation(format!("Unknown schema: {}", schema_name)))?;

        let response_size = serde_json::to_string(response)
            .map(|s| s.len())
            .unwrap_or(0);

        // Perform JSON Schema validation
        let (valid, errors) = self.validate_against_schema(response, &schema.schema);

        let validation_time = start_time.elapsed().as_micros() as u64;

        let result = ValidationResult {
            valid,
            errors,
            schema_name: schema_name.to_string(),
            response_size,
            validation_time_us: validation_time,
        };

        // Emit telemetry
        self.emit_validation_telemetry(&result).await;

        Ok(result)
    }

    /// Validate response against schema (simplified JSON Schema validation)
    fn validate_against_schema(&self, response: &Value, schema: &Value) -> (bool, Vec<String>) {
        let mut errors = Vec::new();

        // Basic validation - check required fields and types
        if let Some(required) = schema.get("required").and_then(|r| r.as_array()) {
            for req_field in required {
                if let Some(field_name) = req_field.as_str() {
                    if !response.get(field_name).is_some() {
                        errors.push(format!("Missing required field: {}", field_name));
                    }
                }
            }
        }

        // Check properties schema
        if let Some(properties) = schema.get("properties").and_then(|p| p.as_object()) {
            for (field_name, field_schema) in properties {
                if let Some(field_value) = response.get(field_name) {
                    if let Some(field_type) = field_schema.get("type").and_then(|t| t.as_str()) {
                        if !self.validate_field_type(field_value, field_type) {
                            errors.push(format!(
                                "Field '{}' has wrong type. Expected {}, got {}",
                                field_name,
                                field_type,
                                self.get_value_type(field_value)
                            ));
                        }
                    }
                }
            }
        }

        (errors.is_empty(), errors)
    }

    /// Validate a field value against expected type
    fn validate_field_type(&self, value: &Value, expected_type: &str) -> bool {
        match expected_type {
            "string" => value.is_string(),
            "number" => value.is_number(),
            "integer" => value.is_i64() || value.is_u64(),
            "boolean" => value.is_boolean(),
            "object" => value.is_object(),
            "array" => value.is_array(),
            "null" => value.is_null(),
            _ => true, // Unknown type, accept
        }
    }

    /// Get string representation of value type
    fn get_value_type(&self, value: &Value) -> &'static str {
        if value.is_string() {
            "string"
        } else if value.is_number() {
            "number"
        } else if value.is_boolean() {
            "boolean"
        } else if value.is_object() {
            "object"
        } else if value.is_array() {
            "array"
        } else if value.is_null() {
            "null"
        } else {
            "unknown"
        }
    }

    /// Register default response schemas
    fn register_default_schemas(&mut self) {
        // Inference response schema
        let inference_schema = ResponseSchema {
            name: "inference_response".to_string(),
            schema: json!({
                "type": "object",
                "required": ["text", "token_count", "latency_ms"],
                "properties": {
                    "text": {"type": "string"},
                    "token_count": {"type": "integer", "minimum": 0},
                    "latency_ms": {"type": "integer", "minimum": 0},
                    "trace": {
                        "type": "object",
                        "properties": {
                            "cpid": {"type": "string"},
                            "input_tokens": {"type": "array", "items": {"type": "integer"}},
                            "generated_tokens": {"type": "array", "items": {"type": "integer"}},
                            "router_decisions": {"type": "array"},
                            "evidence": {"type": "array", "items": {"type": "string"}}
                        }
                    }
                }
            }),
            required: true,
            version: "1.0.0".to_string(),
        };

        // Model list response schema
        let model_list_schema = ResponseSchema {
            name: "model_list_response".to_string(),
            schema: json!({
                "type": "object",
                "required": ["models"],
                "properties": {
                    "models": {
                        "type": "array",
                        "items": {
                            "type": "object",
                            "required": ["id", "name", "status"],
                            "properties": {
                                "id": {"type": "string"},
                                "name": {"type": "string"},
                                "status": {"type": "string"},
                                "created_at": {"type": "string"},
                                "updated_at": {"type": "string"}
                            }
                        }
                    }
                }
            }),
            required: true,
            version: "1.0.0".to_string(),
        };

        // Error response schema
        let error_schema = ResponseSchema {
            name: "error_response".to_string(),
            schema: json!({
                "type": "object",
                "required": ["error"],
                "properties": {
                    "error": {
                        "type": "object",
                        "required": ["message"],
                        "properties": {
                            "message": {"type": "string"},
                            "code": {"type": "string"},
                            "details": {"type": "object"}
                        }
                    }
                }
            }),
            required: true,
            version: "1.0.0".to_string(),
        };

        // Register schemas (ignore errors for defaults)
        let _ = self.register_schema(inference_schema);
        let _ = self.register_schema(model_list_schema);
        let _ = self.register_schema(error_schema);
    }

    /// Emit validation telemetry
    async fn emit_validation_telemetry(&self, result: &ValidationResult) {
        if let Some(ref telemetry) = self.telemetry {
            let event = if result.valid {
                schema_validation_success(
                    &result.schema_name,
                    result.response_size,
                    result.validation_time_us,
                )
            } else {
                schema_validation_failure(
                    &result.schema_name,
                    &result.errors.join("; "),
                    result.response_size,
                    result.validation_time_us,
                )
            };

            match event {
                Ok(e) => {
                    if let Err(log_err) = telemetry.log_event(e) {
                        tracing::warn!("Failed to log validation telemetry: {}", log_err);
                    }
                }
                Err(build_err) => {
                    tracing::warn!("Failed to build validation telemetry event: {}", build_err);
                }
            }
        }
    }

    /// Get all registered schema names
    pub fn get_schema_names(&self) -> Vec<String> {
        self.schemas.keys().cloned().collect()
    }

    /// Check if a schema is registered
    pub fn has_schema(&self, name: &str) -> bool {
        self.schemas.contains_key(name)
    }
}

/// Thread-safe wrapper for response schema validator
pub type SharedResponseValidator = Arc<ResponseSchemaValidator>;

/// Response validation middleware
pub struct ResponseValidationMiddleware {
    validator: SharedResponseValidator,
}

impl ResponseValidationMiddleware {
    /// Create new middleware
    pub fn new(validator: SharedResponseValidator) -> Self {
        Self { validator }
    }

    /// Validate a response and return error if invalid
    pub async fn validate_and_handle(&self, response: &Value, schema_name: &str) -> Result<()> {
        let validation_result = self
            .validator
            .validate_response(response, schema_name)
            .await?;

        if !validation_result.valid {
            return Err(AosError::Validation(format!(
                "Response schema validation failed for '{}': {}",
                schema_name,
                validation_result.errors.join(", ")
            )));
        }

        Ok(())
    }

    /// Validate a response but allow it to proceed even if invalid (for monitoring)
    pub async fn validate_monitor_only(
        &self,
        response: &Value,
        schema_name: &str,
    ) -> ValidationResult {
        self.validator
            .validate_response(response, schema_name)
            .await
            .unwrap_or_else(|_| ValidationResult {
                valid: false,
                errors: vec!["Validation system error".to_string()],
                schema_name: schema_name.to_string(),
                response_size: 0,
                validation_time_us: 0,
            })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn test_response_schema_registration() {
        let mut validator = ResponseSchemaValidator::new(None);

        let schema = ResponseSchema {
            name: "test_schema".to_string(),
            schema: json!({
                "type": "object",
                "required": ["id", "name"],
                "properties": {
                    "id": {"type": "integer"},
                    "name": {"type": "string"}
                }
            }),
            required: true,
            version: "1.0.0".to_string(),
        };

        assert!(validator.register_schema(schema).is_ok());
        assert!(validator.has_schema("test_schema"));
        assert!(!validator.has_schema("nonexistent"));
    }

    #[test]
    fn test_duplicate_schema_registration() {
        let mut validator = ResponseSchemaValidator::new(None);

        let schema1 = ResponseSchema {
            name: "duplicate".to_string(),
            schema: json!({"type": "object"}),
            required: true,
            version: "1.0.0".to_string(),
        };

        let schema2 = ResponseSchema {
            name: "duplicate".to_string(),
            schema: json!({"type": "object"}),
            required: true,
            version: "1.0.0".to_string(),
        };

        assert!(validator.register_schema(schema1).is_ok());
        assert!(validator.register_schema(schema2).is_err());
    }

    #[tokio::test]
    async fn test_valid_inference_response() {
        let validator = ResponseSchemaValidator::new(None);

        let response = json!({
            "text": "Hello world",
            "token_count": 10,
            "latency_ms": 150,
            "trace": {
                "cpid": "test-123",
                "input_tokens": [1, 2, 3],
                "generated_tokens": [4, 5, 6],
                "router_decisions": [],
                "evidence": ["doc1", "doc2"]
            }
        });

        let result = validator
            .validate_response(&response, "inference_response")
            .await;
        assert!(result.is_ok());
        assert!(result.expect("Validation failed").valid);
    }

    #[tokio::test]
    async fn test_invalid_inference_response_missing_required() {
        let validator = ResponseSchemaValidator::new(None);

        let response = json!({
            "text": "Hello world"
            // Missing token_count and latency_ms
        });

        let result = validator
            .validate_response(&response, "inference_response")
            .await;
        assert!(result.is_ok());
        let validation = result.expect("Validation failed");
        assert!(!validation.valid);
        assert!(validation.errors.len() >= 2); // Should have errors for missing fields
    }

    #[tokio::test]
    async fn test_invalid_inference_response_wrong_type() {
        let validator = ResponseSchemaValidator::new(None);

        let response = json!({
            "text": "Hello world",
            "token_count": "not_a_number", // Should be integer
            "latency_ms": 150
        });

        let result = validator
            .validate_response(&response, "inference_response")
            .await;
        assert!(result.is_ok());
        let validation = result.expect("Validation failed");
        assert!(!validation.valid);
        assert!(validation.errors.iter().any(|e| e.contains("token_count")));
    }

    #[tokio::test]
    async fn test_unknown_schema() {
        let validator = ResponseSchemaValidator::new(None);

        let response = json!({"test": "value"});

        let result = validator
            .validate_response(&response, "unknown_schema")
            .await;
        assert!(result.is_err());
    }

    #[test]
    fn test_default_schemas_registered() {
        let validator = ResponseSchemaValidator::new(None);

        assert!(validator.has_schema("inference_response"));
        assert!(validator.has_schema("model_list_response"));
        assert!(validator.has_schema("error_response"));

        let names = validator.get_schema_names();
        assert!(names.contains(&"inference_response".to_string()));
        assert!(names.contains(&"model_list_response".to_string()));
        assert!(names.contains(&"error_response".to_string()));
    }
}
