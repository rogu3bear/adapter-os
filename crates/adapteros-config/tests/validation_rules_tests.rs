//! Comprehensive tests for configuration validation rules
//! Tests: ip_address, url, range, enum validation rules

use adapteros_config::precedence::ConfigBuilder;
use adapteros_config::types::{ConfigSchema, FieldDefinition};

fn empty_schema() -> ConfigSchema {
    ConfigSchema {
        version: "test".to_string(),
        fields: std::collections::HashMap::new(),
    }
}

#[test]
fn test_ip_address_validation_ipv4_valid() {
    let mut schema = empty_schema();
    schema.fields.insert(
        "server.host".to_string(),
        FieldDefinition {
            field_type: "string".to_string(),
            required: false,
            default_value: None,
            description: Some("Server IP".to_string()),
            validation_rules: Some(vec!["ip_address".to_string()]),
        },
    );

    let config = ConfigBuilder::new()
        .with_schema(schema)
        .add_value(
            "server.host".to_string(),
            "192.168.1.1".to_string(),
            adapteros_config::types::PrecedenceLevel::Manifest,
            "test".to_string(),
        )
        .build();

    assert!(config.is_ok(), "Valid IPv4 should pass validation");
}

#[test]
fn test_ip_address_validation_ipv6_valid() {
    let mut schema = empty_schema();
    schema.fields.insert(
        "server.host".to_string(),
        FieldDefinition {
            field_type: "string".to_string(),
            required: false,
            default_value: None,
            description: Some("Server IP".to_string()),
            validation_rules: Some(vec!["ip_address".to_string()]),
        },
    );

    let config = ConfigBuilder::new()
        .with_schema(schema)
        .add_value(
            "server.host".to_string(),
            "2001:db8::1".to_string(),
            adapteros_config::types::PrecedenceLevel::Manifest,
            "test".to_string(),
        )
        .build();

    assert!(config.is_ok(), "Valid IPv6 should pass validation");
}

#[test]
fn test_ip_address_validation_localhost_valid() {
    let mut schema = empty_schema();
    schema.fields.insert(
        "server.host".to_string(),
        FieldDefinition {
            field_type: "string".to_string(),
            required: false,
            default_value: None,
            description: Some("Server IP".to_string()),
            validation_rules: Some(vec!["ip_address".to_string()]),
        },
    );

    let config = ConfigBuilder::new()
        .with_schema(schema)
        .add_value(
            "server.host".to_string(),
            "127.0.0.1".to_string(),
            adapteros_config::types::PrecedenceLevel::Manifest,
            "test".to_string(),
        )
        .build();

    assert!(config.is_ok(), "Localhost should pass validation");
}

#[test]
fn test_ip_address_validation_invalid() {
    let mut schema = empty_schema();
    schema.fields.insert(
        "server.host".to_string(),
        FieldDefinition {
            field_type: "string".to_string(),
            required: false,
            default_value: None,
            description: Some("Server IP".to_string()),
            validation_rules: Some(vec!["ip_address".to_string()]),
        },
    );

    let result = ConfigBuilder::new()
        .with_schema(schema)
        .add_value(
            "server.host".to_string(),
            "not.an.ip.address".to_string(),
            adapteros_config::types::PrecedenceLevel::Manifest,
            "test".to_string(),
        )
        .build();

    assert!(result.is_err(), "Invalid IP should fail validation");
    if let Err(e) = result {
        let error_msg = e.to_string();
        assert!(
            error_msg.contains("IP address"),
            "Error message should mention IP address"
        );
    }
}

#[test]
fn test_url_validation_https_valid() {
    let mut schema = empty_schema();
    schema.fields.insert(
        "api.url".to_string(),
        FieldDefinition {
            field_type: "string".to_string(),
            required: false,
            default_value: None,
            description: Some("API URL".to_string()),
            validation_rules: Some(vec!["url".to_string()]),
        },
    );

    let config = ConfigBuilder::new()
        .with_schema(schema)
        .add_value(
            "api.url".to_string(),
            "https://api.example.com".to_string(),
            adapteros_config::types::PrecedenceLevel::Manifest,
            "test".to_string(),
        )
        .build();

    assert!(config.is_ok(), "Valid HTTPS URL should pass validation");
}

#[test]
fn test_url_validation_http_valid() {
    let mut schema = empty_schema();
    schema.fields.insert(
        "api.url".to_string(),
        FieldDefinition {
            field_type: "string".to_string(),
            required: false,
            default_value: None,
            description: Some("API URL".to_string()),
            validation_rules: Some(vec!["url".to_string()]),
        },
    );

    let config = ConfigBuilder::new()
        .with_schema(schema)
        .add_value(
            "api.url".to_string(),
            "http://localhost:8080".to_string(),
            adapteros_config::types::PrecedenceLevel::Manifest,
            "test".to_string(),
        )
        .build();

    assert!(config.is_ok(), "Valid HTTP URL should pass validation");
}

#[test]
fn test_url_validation_sqlite_valid_absolute_path() {
    let mut schema = empty_schema();
    schema.fields.insert(
        "database.url".to_string(),
        FieldDefinition {
            field_type: "string".to_string(),
            required: false,
            default_value: None,
            description: Some("Database URL".to_string()),
            validation_rules: Some(vec!["url".to_string()]),
        },
    );

    let config = ConfigBuilder::new()
        .with_schema(schema)
        .add_value(
            "database.url".to_string(),
            "sqlite://var/aos-cp.sqlite3".to_string(),
            adapteros_config::types::PrecedenceLevel::Manifest,
            "test".to_string(),
        )
        .build();

    assert!(config.is_ok(), "Valid SQLite URL should pass validation");
}

#[test]
fn test_url_validation_sqlite_valid() {
    let mut schema = empty_schema();
    schema.fields.insert(
        "database.url".to_string(),
        FieldDefinition {
            field_type: "string".to_string(),
            required: false,
            default_value: None,
            description: Some("Database URL".to_string()),
            validation_rules: Some(vec!["url".to_string()]),
        },
    );

    let config = ConfigBuilder::new()
        .with_schema(schema)
        .add_value(
            "database.url".to_string(),
            "sqlite:///path/to/database.db".to_string(),
            adapteros_config::types::PrecedenceLevel::Manifest,
            "test".to_string(),
        )
        .build();

    assert!(config.is_ok(), "Valid SQLite URL should pass validation");
}

#[test]
fn test_url_validation_invalid() {
    let mut schema = empty_schema();
    schema.fields.insert(
        "api.url".to_string(),
        FieldDefinition {
            field_type: "string".to_string(),
            required: false,
            default_value: None,
            description: Some("API URL".to_string()),
            validation_rules: Some(vec!["url".to_string()]),
        },
    );

    let result = ConfigBuilder::new()
        .with_schema(schema)
        .add_value(
            "api.url".to_string(),
            "not a valid url".to_string(),
            adapteros_config::types::PrecedenceLevel::Manifest,
            "test".to_string(),
        )
        .build();

    assert!(result.is_err(), "Invalid URL should fail validation");
    if let Err(e) = result {
        let error_msg = e.to_string();
        assert!(
            error_msg.contains("URL"),
            "Error message should mention URL"
        );
    }
}

#[test]
fn test_range_validation_integer_valid_lower_bound() {
    let mut schema = empty_schema();
    schema.fields.insert(
        "server.port".to_string(),
        FieldDefinition {
            field_type: "integer".to_string(),
            required: false,
            default_value: None,
            description: Some("Server port".to_string()),
            validation_rules: Some(vec!["range:1-65535".to_string()]),
        },
    );

    let config = ConfigBuilder::new()
        .with_schema(schema)
        .add_value(
            "server.port".to_string(),
            "1".to_string(),
            adapteros_config::types::PrecedenceLevel::Manifest,
            "test".to_string(),
        )
        .build();

    assert!(config.is_ok(), "Port at lower bound should pass validation");
}

#[test]
fn test_range_validation_integer_valid_upper_bound() {
    let mut schema = empty_schema();
    schema.fields.insert(
        "server.port".to_string(),
        FieldDefinition {
            field_type: "integer".to_string(),
            required: false,
            default_value: None,
            description: Some("Server port".to_string()),
            validation_rules: Some(vec!["range:1-65535".to_string()]),
        },
    );

    let config = ConfigBuilder::new()
        .with_schema(schema)
        .add_value(
            "server.port".to_string(),
            "65535".to_string(),
            adapteros_config::types::PrecedenceLevel::Manifest,
            "test".to_string(),
        )
        .build();

    assert!(config.is_ok(), "Port at upper bound should pass validation");
}

#[test]
fn test_range_validation_integer_valid_middle() {
    let mut schema = empty_schema();
    schema.fields.insert(
        "server.port".to_string(),
        FieldDefinition {
            field_type: "integer".to_string(),
            required: false,
            default_value: None,
            description: Some("Server port".to_string()),
            validation_rules: Some(vec!["range:1-65535".to_string()]),
        },
    );

    let config = ConfigBuilder::new()
        .with_schema(schema)
        .add_value(
            "server.port".to_string(),
            "8080".to_string(),
            adapteros_config::types::PrecedenceLevel::Manifest,
            "test".to_string(),
        )
        .build();

    assert!(config.is_ok(), "Port in valid range should pass validation");
}

#[test]
fn test_range_validation_integer_below_min() {
    let mut schema = empty_schema();
    schema.fields.insert(
        "server.port".to_string(),
        FieldDefinition {
            field_type: "integer".to_string(),
            required: false,
            default_value: None,
            description: Some("Server port".to_string()),
            validation_rules: Some(vec!["range:1-65535".to_string()]),
        },
    );

    let result = ConfigBuilder::new()
        .with_schema(schema)
        .add_value(
            "server.port".to_string(),
            "0".to_string(),
            adapteros_config::types::PrecedenceLevel::Manifest,
            "test".to_string(),
        )
        .build();

    assert!(result.is_err(), "Port below minimum should fail validation");
    if let Err(e) = result {
        let error_msg = e.to_string();
        assert!(
            error_msg.contains("range"),
            "Error message should mention range"
        );
    }
}

#[test]
fn test_range_validation_integer_above_max() {
    let mut schema = empty_schema();
    schema.fields.insert(
        "server.port".to_string(),
        FieldDefinition {
            field_type: "integer".to_string(),
            required: false,
            default_value: None,
            description: Some("Server port".to_string()),
            validation_rules: Some(vec!["range:1-65535".to_string()]),
        },
    );

    let result = ConfigBuilder::new()
        .with_schema(schema)
        .add_value(
            "server.port".to_string(),
            "65536".to_string(),
            adapteros_config::types::PrecedenceLevel::Manifest,
            "test".to_string(),
        )
        .build();

    assert!(result.is_err(), "Port above maximum should fail validation");
    if let Err(e) = result {
        let error_msg = e.to_string();
        assert!(
            error_msg.contains("range"),
            "Error message should mention range"
        );
    }
}

#[test]
fn test_enum_validation_string_valid_first() {
    let mut schema = empty_schema();
    schema.fields.insert(
        "logging.level".to_string(),
        FieldDefinition {
            field_type: "string".to_string(),
            required: false,
            default_value: None,
            description: Some("Logging level".to_string()),
            validation_rules: Some(vec!["enum:debug,info,warn,error".to_string()]),
        },
    );

    let config = ConfigBuilder::new()
        .with_schema(schema)
        .add_value(
            "logging.level".to_string(),
            "debug".to_string(),
            adapteros_config::types::PrecedenceLevel::Manifest,
            "test".to_string(),
        )
        .build();

    assert!(config.is_ok(), "First enum value should pass validation");
}

#[test]
fn test_enum_validation_string_valid_middle() {
    let mut schema = empty_schema();
    schema.fields.insert(
        "logging.level".to_string(),
        FieldDefinition {
            field_type: "string".to_string(),
            required: false,
            default_value: None,
            description: Some("Logging level".to_string()),
            validation_rules: Some(vec!["enum:debug,info,warn,error".to_string()]),
        },
    );

    let config = ConfigBuilder::new()
        .with_schema(schema)
        .add_value(
            "logging.level".to_string(),
            "info".to_string(),
            adapteros_config::types::PrecedenceLevel::Manifest,
            "test".to_string(),
        )
        .build();

    assert!(config.is_ok(), "Middle enum value should pass validation");
}

#[test]
fn test_enum_validation_string_valid_last() {
    let mut schema = empty_schema();
    schema.fields.insert(
        "logging.level".to_string(),
        FieldDefinition {
            field_type: "string".to_string(),
            required: false,
            default_value: None,
            description: Some("Logging level".to_string()),
            validation_rules: Some(vec!["enum:debug,info,warn,error".to_string()]),
        },
    );

    let config = ConfigBuilder::new()
        .with_schema(schema)
        .add_value(
            "logging.level".to_string(),
            "error".to_string(),
            adapteros_config::types::PrecedenceLevel::Manifest,
            "test".to_string(),
        )
        .build();

    assert!(config.is_ok(), "Last enum value should pass validation");
}

#[test]
fn test_enum_validation_string_invalid() {
    let mut schema = empty_schema();
    schema.fields.insert(
        "logging.level".to_string(),
        FieldDefinition {
            field_type: "string".to_string(),
            required: false,
            default_value: None,
            description: Some("Logging level".to_string()),
            validation_rules: Some(vec!["enum:debug,info,warn,error".to_string()]),
        },
    );

    let result = ConfigBuilder::new()
        .with_schema(schema)
        .add_value(
            "logging.level".to_string(),
            "invalid".to_string(),
            adapteros_config::types::PrecedenceLevel::Manifest,
            "test".to_string(),
        )
        .build();

    assert!(result.is_err(), "Invalid enum value should fail validation");
    if let Err(e) = result {
        let error_msg = e.to_string();
        assert!(
            error_msg.contains("one of"),
            "Error message should mention allowed values"
        );
    }
}

#[test]
fn test_enum_validation_string_case_sensitive() {
    let mut schema = empty_schema();
    schema.fields.insert(
        "logging.level".to_string(),
        FieldDefinition {
            field_type: "string".to_string(),
            required: false,
            default_value: None,
            description: Some("Logging level".to_string()),
            validation_rules: Some(vec!["enum:debug,info,warn,error".to_string()]),
        },
    );

    let result = ConfigBuilder::new()
        .with_schema(schema)
        .add_value(
            "logging.level".to_string(),
            "DEBUG".to_string(),
            adapteros_config::types::PrecedenceLevel::Manifest,
            "test".to_string(),
        )
        .build();

    assert!(result.is_err(), "Enum validation should be case-sensitive");
}

#[test]
fn test_multiple_validation_rules_string_min_length_and_enum() {
    let mut schema = empty_schema();
    schema.fields.insert(
        "logging.format".to_string(),
        FieldDefinition {
            field_type: "string".to_string(),
            required: false,
            default_value: None,
            description: Some("Logging format".to_string()),
            validation_rules: Some(vec![
                "min_length:3".to_string(),
                "enum:json,text".to_string(),
            ]),
        },
    );

    let config = ConfigBuilder::new()
        .with_schema(schema)
        .add_value(
            "logging.format".to_string(),
            "json".to_string(),
            adapteros_config::types::PrecedenceLevel::Manifest,
            "test".to_string(),
        )
        .build();

    assert!(
        config.is_ok(),
        "Value passing all rules should pass validation"
    );
}

#[test]
fn test_range_validation_with_negative_numbers() {
    let mut schema = empty_schema();
    schema.fields.insert(
        "threshold".to_string(),
        FieldDefinition {
            field_type: "integer".to_string(),
            required: false,
            default_value: None,
            description: Some("Threshold value".to_string()),
            validation_rules: Some(vec!["range:-100-100".to_string()]),
        },
    );

    let config = ConfigBuilder::new()
        .with_schema(schema)
        .add_value(
            "threshold".to_string(),
            "-50".to_string(),
            adapteros_config::types::PrecedenceLevel::Manifest,
            "test".to_string(),
        )
        .build();

    assert!(config.is_ok(), "Negative value in valid range should pass");
}

#[test]
fn test_workers_range_validation() {
    let mut schema = empty_schema();
    schema.fields.insert(
        "server.workers".to_string(),
        FieldDefinition {
            field_type: "integer".to_string(),
            required: false,
            default_value: Some("4".to_string()),
            description: Some("Number of worker threads".to_string()),
            validation_rules: Some(vec!["range:1-64".to_string()]),
        },
    );

    let config = ConfigBuilder::new()
        .with_schema(schema)
        .add_value(
            "server.workers".to_string(),
            "8".to_string(),
            adapteros_config::types::PrecedenceLevel::Manifest,
            "test".to_string(),
        )
        .build();

    assert!(config.is_ok(), "Worker count in valid range should pass");
}

#[test]
fn test_enum_validation_json_format() {
    let mut schema = empty_schema();
    schema.fields.insert(
        "logging.format".to_string(),
        FieldDefinition {
            field_type: "string".to_string(),
            required: false,
            default_value: Some("json".to_string()),
            description: Some("Logging format".to_string()),
            validation_rules: Some(vec!["enum:json,text".to_string()]),
        },
    );

    let config = ConfigBuilder::new()
        .with_schema(schema)
        .add_value(
            "logging.format".to_string(),
            "json".to_string(),
            adapteros_config::types::PrecedenceLevel::Manifest,
            "test".to_string(),
        )
        .build();

    assert!(config.is_ok(), "JSON format should pass enum validation");
}
