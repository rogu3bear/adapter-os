//! Migration validation utilities
//!
//! This module provides functions for validating database migrations:
//!
//! - **Checksum verification** - Ensures migration file contents match expected hashes
//! - **Ordering validation** - Ensures migrations are applied in correct sequence
//! - **Schema version checking** - Compares app vs database schema versions
//!
//! # Usage
//!
//! ```ignore
//! use adapteros_db::migration_validation::{
//!     verify_migration_checksum,
//!     validate_migration_order,
//!     check_schema_version,
//!     VersionCheck,
//! };
//!
//! // Verify a migration file's integrity
//! verify_migration_checksum("V001__init.sql", migration_content, expected_hash)?;
//!
//! // Validate migration ordering
//! validate_migration_order(&applied_migrations)?;
//!
//! // Check schema compatibility
//! match check_schema_version(expected_version, db_version)? {
//!     VersionCheck::Compatible => println!("Schema is compatible"),
//!     VersionCheck::NeedsMigration { from, to } => {
//!         println!("Migration needed from {} to {}", from, to);
//!     }
//! }
//! ```

use adapteros_core::errors::storage::AosStorageError;
use blake3::Hasher;

/// Result of a schema version compatibility check
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum VersionCheck {
    /// Schema versions are compatible, no action needed
    Compatible,
    /// Migration is needed to bring database up to date
    NeedsMigration {
        /// Current database version
        from: i64,
        /// Target application version
        to: i64,
    },
    /// Database is ahead of application (possible downgrade scenario)
    DatabaseAhead {
        /// Application's expected version
        app_version: i64,
        /// Database's current version
        db_version: i64,
    },
}

/// Information about an applied migration for ordering validation
#[derive(Debug, Clone)]
pub struct AppliedMigration {
    /// The migration filename (e.g., "V001__init.sql")
    pub filename: String,
    /// The version number extracted from the filename
    pub version: i64,
    /// When this migration was applied (as unix timestamp or ordinal)
    pub applied_at: i64,
}

/// Verifies that a migration file's content matches the expected checksum.
///
/// This function computes a BLAKE3 hash of the migration content and compares
/// it to the expected hash. This ensures migrations haven't been modified
/// after being applied.
///
/// # Arguments
///
/// * `filename` - The migration filename (for error messages)
/// * `content` - The migration file content
/// * `expected_hash` - The expected BLAKE3 hash in hex format
///
/// # Returns
///
/// * `Ok(())` if the checksum matches
/// * `Err(AosStorageError::MigrationChecksumMismatch)` if checksums differ
///
/// # Example
///
/// ```ignore
/// verify_migration_checksum(
///     "V001__create_users.sql",
///     "CREATE TABLE users (id INTEGER PRIMARY KEY);",
///     "a1b2c3d4...",
/// )?;
/// ```
pub fn verify_migration_checksum(
    filename: &str,
    content: &str,
    expected_hash: &str,
) -> Result<(), AosStorageError> {
    let mut hasher = Hasher::new();
    hasher.update(content.as_bytes());
    let computed = hasher.finalize();
    let computed_hex = hex::encode(computed.as_bytes());

    if computed_hex != expected_hash {
        return Err(AosStorageError::MigrationChecksumMismatch {
            filename: filename.to_string(),
            expected: expected_hash.to_string(),
            actual: computed_hex,
        });
    }

    Ok(())
}

/// Validates that migrations were applied in the correct version order.
///
/// This function checks that:
/// 1. Migration versions are strictly increasing
/// 2. No migrations were applied out of sequence
///
/// # Arguments
///
/// * `migrations` - Slice of applied migrations, ordered by `applied_at`
///
/// # Returns
///
/// * `Ok(())` if migrations are in correct order
/// * `Err(AosStorageError::MigrationOutOfOrder)` if a migration was applied out of sequence
///
/// # Example
///
/// ```ignore
/// let migrations = vec![
///     AppliedMigration { filename: "V001__init.sql".into(), version: 1, applied_at: 1000 },
///     AppliedMigration { filename: "V002__users.sql".into(), version: 2, applied_at: 1001 },
/// ];
/// validate_migration_order(&migrations)?;
/// ```
pub fn validate_migration_order(migrations: &[AppliedMigration]) -> Result<(), AosStorageError> {
    if migrations.is_empty() {
        return Ok(());
    }

    // Check that versions are strictly increasing when sorted by applied_at
    let mut sorted = migrations.to_vec();
    sorted.sort_by_key(|m| m.applied_at);

    let mut last_version = 0i64;
    for migration in &sorted {
        if migration.version <= last_version {
            return Err(AosStorageError::MigrationOutOfOrder {
                filename: migration.filename.clone(),
                version: migration.version,
                applied_after: last_version,
            });
        }
        last_version = migration.version;
    }

    Ok(())
}

/// Checks schema version compatibility between application and database.
///
/// This function compares the application's expected schema version with
/// the database's current version to determine what action (if any) is needed.
///
/// # Arguments
///
/// * `app_version` - The schema version the application expects
/// * `db_version` - The schema version currently in the database
///
/// # Returns
///
/// * `Ok(VersionCheck::Compatible)` if versions match
/// * `Ok(VersionCheck::NeedsMigration { from, to })` if database needs updating
/// * `Ok(VersionCheck::DatabaseAhead { ... })` if database is ahead of application
/// * `Err(AosStorageError::SchemaVersionMismatch)` if the mismatch is critical
///
/// # Example
///
/// ```ignore
/// match check_schema_version(5, 3)? {
///     VersionCheck::Compatible => {},
///     VersionCheck::NeedsMigration { from, to } => {
///         run_migrations(from, to)?;
///     },
///     VersionCheck::DatabaseAhead { .. } => {
///         // Handle downgrade scenario
///     }
/// }
/// ```
pub fn check_schema_version(
    app_version: i64,
    db_version: i64,
) -> Result<VersionCheck, AosStorageError> {
    match app_version.cmp(&db_version) {
        std::cmp::Ordering::Equal => Ok(VersionCheck::Compatible),
        std::cmp::Ordering::Greater => {
            // Database is behind - needs migration
            Ok(VersionCheck::NeedsMigration {
                from: db_version,
                to: app_version,
            })
        }
        std::cmp::Ordering::Less => {
            // Database is ahead - possible downgrade scenario
            // This is typically an error condition
            Ok(VersionCheck::DatabaseAhead {
                app_version,
                db_version,
            })
        }
    }
}

/// Checks schema version compatibility and returns an error for critical mismatches.
///
/// Unlike `check_schema_version`, this function returns an error if the database
/// is ahead of the application version, as this typically indicates a deployment
/// issue where an older application is connecting to a newer database.
///
/// # Arguments
///
/// * `app_version` - The schema version the application expects
/// * `db_version` - The schema version currently in the database
///
/// # Returns
///
/// * `Ok(VersionCheck)` for compatible scenarios
/// * `Err(AosStorageError::SchemaVersionMismatch)` if database is ahead
pub fn check_schema_version_strict(
    app_version: i64,
    db_version: i64,
) -> Result<VersionCheck, AosStorageError> {
    let check = check_schema_version(app_version, db_version)?;

    if let VersionCheck::DatabaseAhead {
        app_version,
        db_version,
    } = check
    {
        return Err(AosStorageError::SchemaVersionMismatch {
            app_version,
            db_version,
            direction: "ahead".to_string(),
        });
    }

    Ok(check)
}

/// Extracts the version number from a migration filename.
///
/// Supports common migration naming conventions:
/// - `V001__description.sql` -> 1
/// - `001_description.sql` -> 1
/// - `20240101120000_description.sql` -> 20240101120000
///
/// # Arguments
///
/// * `filename` - The migration filename
///
/// # Returns
///
/// * `Some(version)` if a version could be extracted
/// * `None` if the filename doesn't match expected patterns
pub fn extract_version_from_filename(filename: &str) -> Option<i64> {
    // Try V prefix format (e.g., V001__init.sql)
    if filename.starts_with('V') || filename.starts_with('v') {
        let rest = &filename[1..];
        if let Some(underscore_pos) = rest.find('_') {
            return rest[..underscore_pos].parse().ok();
        }
    }

    // Try leading digits format (e.g., 001_init.sql or 20240101_init.sql)
    let leading_digits: String = filename
        .chars()
        .take_while(|c| c.is_ascii_digit())
        .collect();
    if !leading_digits.is_empty() {
        return leading_digits.parse().ok();
    }

    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_verify_migration_checksum_success() {
        let content = "CREATE TABLE users (id INTEGER PRIMARY KEY);";
        let mut hasher = Hasher::new();
        hasher.update(content.as_bytes());
        let expected = hex::encode(hasher.finalize().as_bytes());

        let result = verify_migration_checksum("V001__init.sql", content, &expected);
        assert!(result.is_ok());
    }

    #[test]
    fn test_verify_migration_checksum_mismatch() {
        let content = "CREATE TABLE users (id INTEGER PRIMARY KEY);";
        let wrong_hash = "0".repeat(64); // Invalid hash

        let result = verify_migration_checksum("V001__init.sql", content, &wrong_hash);
        assert!(matches!(
            result,
            Err(AosStorageError::MigrationChecksumMismatch { .. })
        ));
    }

    #[test]
    fn test_validate_migration_order_success() {
        let migrations = vec![
            AppliedMigration {
                filename: "V001__init.sql".into(),
                version: 1,
                applied_at: 1000,
            },
            AppliedMigration {
                filename: "V002__users.sql".into(),
                version: 2,
                applied_at: 1001,
            },
            AppliedMigration {
                filename: "V003__posts.sql".into(),
                version: 3,
                applied_at: 1002,
            },
        ];

        let result = validate_migration_order(&migrations);
        assert!(result.is_ok());
    }

    #[test]
    fn test_validate_migration_order_out_of_order() {
        let migrations = vec![
            AppliedMigration {
                filename: "V001__init.sql".into(),
                version: 1,
                applied_at: 1000,
            },
            AppliedMigration {
                filename: "V003__posts.sql".into(),
                version: 3,
                applied_at: 1001,
            },
            AppliedMigration {
                filename: "V002__users.sql".into(),
                version: 2,
                applied_at: 1002, // Applied after V003 but has lower version
            },
        ];

        let result = validate_migration_order(&migrations);
        assert!(matches!(
            result,
            Err(AosStorageError::MigrationOutOfOrder { .. })
        ));
    }

    #[test]
    fn test_validate_migration_order_empty() {
        let result = validate_migration_order(&[]);
        assert!(result.is_ok());
    }

    #[test]
    fn test_check_schema_version_compatible() {
        let result = check_schema_version(5, 5).unwrap();
        assert_eq!(result, VersionCheck::Compatible);
    }

    #[test]
    fn test_check_schema_version_needs_migration() {
        let result = check_schema_version(5, 3).unwrap();
        assert_eq!(result, VersionCheck::NeedsMigration { from: 3, to: 5 });
    }

    #[test]
    fn test_check_schema_version_database_ahead() {
        let result = check_schema_version(3, 5).unwrap();
        assert_eq!(
            result,
            VersionCheck::DatabaseAhead {
                app_version: 3,
                db_version: 5,
            }
        );
    }

    #[test]
    fn test_check_schema_version_strict_error() {
        let result = check_schema_version_strict(3, 5);
        assert!(matches!(
            result,
            Err(AosStorageError::SchemaVersionMismatch { .. })
        ));
    }

    #[test]
    fn test_extract_version_from_filename() {
        assert_eq!(extract_version_from_filename("V001__init.sql"), Some(1));
        assert_eq!(extract_version_from_filename("V123__users.sql"), Some(123));
        assert_eq!(extract_version_from_filename("v001__init.sql"), Some(1));
        assert_eq!(extract_version_from_filename("001_init.sql"), Some(1));
        assert_eq!(
            extract_version_from_filename("20240101120000_init.sql"),
            Some(20240101120000)
        );
        assert_eq!(extract_version_from_filename("invalid.sql"), None);
    }

    // =========================================================================
    // Edge cases for version extraction (unicode, special chars)
    // =========================================================================

    #[test]
    fn test_extract_version_unicode_in_description() {
        // Unicode characters in description should not affect version extraction
        assert_eq!(extract_version_from_filename("V001__日本語.sql"), Some(1));
        assert_eq!(
            extract_version_from_filename("V042__émigration.sql"),
            Some(42)
        );
        assert_eq!(
            extract_version_from_filename("V999__数据库迁移.sql"),
            Some(999)
        );
        assert_eq!(extract_version_from_filename("001_кириллица.sql"), Some(1));
    }

    #[test]
    fn test_extract_version_special_characters_in_description() {
        // Special characters after the version number
        assert_eq!(
            extract_version_from_filename("V001__add-users.sql"),
            Some(1)
        );
        assert_eq!(
            extract_version_from_filename("V002__add_users.sql"),
            Some(2)
        );
        assert_eq!(
            extract_version_from_filename("V003__add.users.sql"),
            Some(3)
        );
        assert_eq!(
            extract_version_from_filename("V004__add@users.sql"),
            Some(4)
        );
        assert_eq!(
            extract_version_from_filename("V005__add#users.sql"),
            Some(5)
        );
        assert_eq!(
            extract_version_from_filename("V006__add$users.sql"),
            Some(6)
        );
        assert_eq!(
            extract_version_from_filename("V007__add%users.sql"),
            Some(7)
        );
    }

    #[test]
    fn test_extract_version_edge_case_empty_description() {
        assert_eq!(extract_version_from_filename("V001__.sql"), Some(1));
        assert_eq!(extract_version_from_filename("001_.sql"), Some(1));
    }

    #[test]
    fn test_extract_version_no_underscore_separator() {
        // V prefix without underscore should return None
        assert_eq!(extract_version_from_filename("V001init.sql"), None);
        assert_eq!(extract_version_from_filename("V001.sql"), None);
    }

    #[test]
    fn test_extract_version_leading_zeros() {
        assert_eq!(extract_version_from_filename("V0001__init.sql"), Some(1));
        assert_eq!(
            extract_version_from_filename("V00000001__init.sql"),
            Some(1)
        );
        assert_eq!(extract_version_from_filename("0001_init.sql"), Some(1));
        assert_eq!(
            extract_version_from_filename("00000100_init.sql"),
            Some(100)
        );
    }

    #[test]
    fn test_extract_version_max_i64_boundary() {
        // Test near i64::MAX boundary
        let max_i64_str = format!("V{}_init.sql", i64::MAX);
        assert_eq!(extract_version_from_filename(&max_i64_str), Some(i64::MAX));

        // Test overflow - should return None (parse fails)
        let overflow_str = "V9223372036854775808_init.sql"; // i64::MAX + 1
        assert_eq!(extract_version_from_filename(overflow_str), None);
    }

    #[test]
    fn test_extract_version_zero() {
        assert_eq!(extract_version_from_filename("V0__init.sql"), Some(0));
        assert_eq!(extract_version_from_filename("V000__init.sql"), Some(0));
        assert_eq!(extract_version_from_filename("0_init.sql"), Some(0));
        assert_eq!(extract_version_from_filename("000_init.sql"), Some(0));
    }

    #[test]
    fn test_extract_version_mixed_case_v_prefix() {
        assert_eq!(extract_version_from_filename("V001__init.sql"), Some(1));
        assert_eq!(extract_version_from_filename("v001__init.sql"), Some(1));
        // Only V or v are supported, not other case variations
    }

    #[test]
    fn test_extract_version_only_digits_no_extension() {
        assert_eq!(extract_version_from_filename("123"), Some(123));
        assert_eq!(extract_version_from_filename("123_"), Some(123));
    }

    #[test]
    fn test_extract_version_whitespace_handling() {
        // Leading whitespace should cause extraction to fail for V prefix
        assert_eq!(extract_version_from_filename(" V001__init.sql"), None);
        // Spaces in description should work fine
        assert_eq!(
            extract_version_from_filename("V001__init file.sql"),
            Some(1)
        );
    }

    #[test]
    fn test_extract_version_empty_filename() {
        assert_eq!(extract_version_from_filename(""), None);
    }

    #[test]
    fn test_extract_version_only_v_prefix() {
        assert_eq!(extract_version_from_filename("V"), None);
        assert_eq!(extract_version_from_filename("v"), None);
        assert_eq!(extract_version_from_filename("V_init.sql"), None);
    }

    // =========================================================================
    // Large migration lists for ordering validation
    // =========================================================================

    #[test]
    fn test_validate_migration_order_large_list_sequential() {
        // Test with 1000 migrations in correct order
        let migrations: Vec<AppliedMigration> = (1..=1000)
            .map(|i| AppliedMigration {
                filename: format!("V{:04}__migration_{}.sql", i, i),
                version: i,
                applied_at: 1000 + i,
            })
            .collect();

        let result = validate_migration_order(&migrations);
        assert!(result.is_ok());
    }

    #[test]
    fn test_validate_migration_order_large_list_with_gaps() {
        // Test with non-sequential version numbers (gaps allowed)
        let migrations: Vec<AppliedMigration> = [1, 5, 10, 50, 100, 500, 1000]
            .iter()
            .enumerate()
            .map(|(idx, &v)| AppliedMigration {
                filename: format!("V{:04}__migration.sql", v),
                version: v,
                applied_at: 1000 + idx as i64,
            })
            .collect();

        let result = validate_migration_order(&migrations);
        assert!(result.is_ok());
    }

    #[test]
    fn test_validate_migration_order_large_list_out_of_order() {
        // Create 100 migrations, then add one out of order at the end
        let mut migrations: Vec<AppliedMigration> = (1..=100)
            .map(|i| AppliedMigration {
                filename: format!("V{:04}__migration_{}.sql", i, i),
                version: i,
                applied_at: 1000 + i,
            })
            .collect();

        // Add migration V50 applied after V100 (out of order)
        migrations.push(AppliedMigration {
            filename: "V0050__late_migration.sql".into(),
            version: 50,
            applied_at: 2000,
        });

        let result = validate_migration_order(&migrations);
        assert!(matches!(
            result,
            Err(AosStorageError::MigrationOutOfOrder {
                version: 50,
                applied_after: 100,
                ..
            })
        ));
    }

    #[test]
    fn test_validate_migration_order_reversed_applied_at() {
        // Migrations applied in reverse order (should fail because versions decrease)
        let migrations: Vec<AppliedMigration> = (1..=10)
            .rev()
            .enumerate()
            .map(|(idx, v)| AppliedMigration {
                filename: format!("V{:04}__migration.sql", v),
                version: v as i64,
                applied_at: 1000 + idx as i64,
            })
            .collect();

        let result = validate_migration_order(&migrations);
        assert!(matches!(
            result,
            Err(AosStorageError::MigrationOutOfOrder { .. })
        ));
    }

    #[test]
    fn test_validate_migration_order_single_migration() {
        let migrations = vec![AppliedMigration {
            filename: "V001__init.sql".into(),
            version: 1,
            applied_at: 1000,
        }];

        let result = validate_migration_order(&migrations);
        assert!(result.is_ok());
    }

    #[test]
    fn test_validate_migration_order_duplicate_versions() {
        // Two migrations with same version applied sequentially
        let migrations = vec![
            AppliedMigration {
                filename: "V001__init.sql".into(),
                version: 1,
                applied_at: 1000,
            },
            AppliedMigration {
                filename: "V001__duplicate.sql".into(),
                version: 1,
                applied_at: 1001,
            },
        ];

        let result = validate_migration_order(&migrations);
        assert!(matches!(
            result,
            Err(AosStorageError::MigrationOutOfOrder {
                version: 1,
                applied_after: 1,
                ..
            })
        ));
    }

    #[test]
    fn test_validate_migration_order_same_applied_at_timestamp() {
        // Multiple migrations with same applied_at timestamp but correct version order
        let migrations = vec![
            AppliedMigration {
                filename: "V001__init.sql".into(),
                version: 1,
                applied_at: 1000,
            },
            AppliedMigration {
                filename: "V002__users.sql".into(),
                version: 2,
                applied_at: 1000, // Same timestamp
            },
            AppliedMigration {
                filename: "V003__posts.sql".into(),
                version: 3,
                applied_at: 1000, // Same timestamp
            },
        ];

        // When applied_at is the same, sort is stable, so original order is preserved
        // Since versions are increasing in the original order, this should pass
        let result = validate_migration_order(&migrations);
        assert!(result.is_ok());
    }

    #[test]
    fn test_validate_migration_order_timestamp_boundary_values() {
        let migrations = vec![
            AppliedMigration {
                filename: "V001__init.sql".into(),
                version: 1,
                applied_at: i64::MIN,
            },
            AppliedMigration {
                filename: "V002__users.sql".into(),
                version: 2,
                applied_at: 0,
            },
            AppliedMigration {
                filename: "V003__posts.sql".into(),
                version: 3,
                applied_at: i64::MAX,
            },
        ];

        let result = validate_migration_order(&migrations);
        assert!(result.is_ok());
    }

    // =========================================================================
    // Boundary conditions for schema version checking
    // =========================================================================

    #[test]
    fn test_check_schema_version_zero_versions() {
        assert_eq!(
            check_schema_version(0, 0).unwrap(),
            VersionCheck::Compatible
        );
    }

    #[test]
    fn test_check_schema_version_zero_app_positive_db() {
        assert_eq!(
            check_schema_version(0, 5).unwrap(),
            VersionCheck::DatabaseAhead {
                app_version: 0,
                db_version: 5,
            }
        );
    }

    #[test]
    fn test_check_schema_version_positive_app_zero_db() {
        assert_eq!(
            check_schema_version(5, 0).unwrap(),
            VersionCheck::NeedsMigration { from: 0, to: 5 }
        );
    }

    #[test]
    fn test_check_schema_version_negative_versions() {
        // Negative versions are technically possible (though unusual)
        assert_eq!(
            check_schema_version(-1, -1).unwrap(),
            VersionCheck::Compatible
        );
        assert_eq!(
            check_schema_version(-1, -5).unwrap(),
            VersionCheck::NeedsMigration { from: -5, to: -1 }
        );
        assert_eq!(
            check_schema_version(-5, -1).unwrap(),
            VersionCheck::DatabaseAhead {
                app_version: -5,
                db_version: -1,
            }
        );
    }

    #[test]
    fn test_check_schema_version_i64_max() {
        assert_eq!(
            check_schema_version(i64::MAX, i64::MAX).unwrap(),
            VersionCheck::Compatible
        );
        assert_eq!(
            check_schema_version(i64::MAX, i64::MAX - 1).unwrap(),
            VersionCheck::NeedsMigration {
                from: i64::MAX - 1,
                to: i64::MAX
            }
        );
    }

    #[test]
    fn test_check_schema_version_i64_min() {
        assert_eq!(
            check_schema_version(i64::MIN, i64::MIN).unwrap(),
            VersionCheck::Compatible
        );
        assert_eq!(
            check_schema_version(i64::MIN + 1, i64::MIN).unwrap(),
            VersionCheck::NeedsMigration {
                from: i64::MIN,
                to: i64::MIN + 1
            }
        );
    }

    #[test]
    fn test_check_schema_version_large_gap() {
        // Very large version gap
        assert_eq!(
            check_schema_version(1_000_000, 1).unwrap(),
            VersionCheck::NeedsMigration {
                from: 1,
                to: 1_000_000
            }
        );
    }

    #[test]
    fn test_check_schema_version_strict_compatible() {
        let result = check_schema_version_strict(5, 5).unwrap();
        assert_eq!(result, VersionCheck::Compatible);
    }

    #[test]
    fn test_check_schema_version_strict_needs_migration() {
        let result = check_schema_version_strict(10, 5).unwrap();
        assert_eq!(result, VersionCheck::NeedsMigration { from: 5, to: 10 });
    }

    #[test]
    fn test_check_schema_version_strict_database_ahead_error() {
        let result = check_schema_version_strict(5, 10);
        match result {
            Err(AosStorageError::SchemaVersionMismatch {
                app_version,
                db_version,
                direction,
            }) => {
                assert_eq!(app_version, 5);
                assert_eq!(db_version, 10);
                assert_eq!(direction, "ahead");
            }
            _ => panic!("Expected SchemaVersionMismatch error"),
        }
    }

    #[test]
    fn test_check_schema_version_strict_boundary_values() {
        // Zero to positive
        assert!(check_schema_version_strict(5, 0).is_ok());

        // Positive to zero (database ahead)
        assert!(check_schema_version_strict(0, 5).is_err());

        // i64::MAX cases
        assert!(check_schema_version_strict(i64::MAX, i64::MAX - 1).is_ok());
        assert!(check_schema_version_strict(i64::MAX - 1, i64::MAX).is_err());
    }

    // =========================================================================
    // Checksum verification with various content types
    // =========================================================================

    fn compute_hash(content: &str) -> String {
        let mut hasher = Hasher::new();
        hasher.update(content.as_bytes());
        hex::encode(hasher.finalize().as_bytes())
    }

    #[test]
    fn test_checksum_empty_content() {
        let content = "";
        let hash = compute_hash(content);
        assert!(verify_migration_checksum("V001__empty.sql", content, &hash).is_ok());
    }

    #[test]
    fn test_checksum_whitespace_only() {
        let content = "   \n\t\r\n   ";
        let hash = compute_hash(content);
        assert!(verify_migration_checksum("V001__whitespace.sql", content, &hash).is_ok());
    }

    #[test]
    fn test_checksum_unicode_content() {
        let content = "-- 日本語コメント\nCREATE TABLE 用户 (名前 TEXT);";
        let hash = compute_hash(content);
        assert!(verify_migration_checksum("V001__unicode.sql", content, &hash).is_ok());
    }

    #[test]
    fn test_checksum_emoji_content() {
        let content = "-- Migration with emojis 🚀🎉\nCREATE TABLE test (id INT);";
        let hash = compute_hash(content);
        assert!(verify_migration_checksum("V001__emoji.sql", content, &hash).is_ok());
    }

    #[test]
    fn test_checksum_binary_like_content() {
        // Content with null bytes and other binary-like characters
        let content = "CREATE TABLE test;\x00\x01\x02\x7f";
        let hash = compute_hash(content);
        assert!(verify_migration_checksum("V001__binary.sql", content, &hash).is_ok());
    }

    #[test]
    fn test_checksum_very_long_content() {
        // 1MB of content
        let content = "CREATE TABLE test;".repeat(50000);
        let hash = compute_hash(&content);
        assert!(verify_migration_checksum("V001__large.sql", &content, &hash).is_ok());
    }

    #[test]
    fn test_checksum_single_character_difference() {
        let content1 = "CREATE TABLE users (id INT);";
        let content2 = "CREATE TABLE users (id INt);"; // lowercase 't'
        let hash1 = compute_hash(content1);

        // Verify original works
        assert!(verify_migration_checksum("V001__test.sql", content1, &hash1).is_ok());

        // Single character difference should fail
        let result = verify_migration_checksum("V001__test.sql", content2, &hash1);
        assert!(matches!(
            result,
            Err(AosStorageError::MigrationChecksumMismatch { .. })
        ));
    }

    #[test]
    fn test_checksum_trailing_newline_difference() {
        let content_with_newline = "CREATE TABLE users (id INT);\n";
        let content_without_newline = "CREATE TABLE users (id INT);";

        let hash_with = compute_hash(content_with_newline);
        let hash_without = compute_hash(content_without_newline);

        // Hashes should be different
        assert_ne!(hash_with, hash_without);

        // Cross-verification should fail
        let result =
            verify_migration_checksum("V001__test.sql", content_without_newline, &hash_with);
        assert!(matches!(
            result,
            Err(AosStorageError::MigrationChecksumMismatch { .. })
        ));
    }

    #[test]
    fn test_checksum_crlf_vs_lf() {
        let content_lf = "CREATE TABLE users;\nINSERT INTO users;";
        let content_crlf = "CREATE TABLE users;\r\nINSERT INTO users;";

        let hash_lf = compute_hash(content_lf);

        // CRLF content with LF hash should fail
        let result = verify_migration_checksum("V001__test.sql", content_crlf, &hash_lf);
        assert!(matches!(
            result,
            Err(AosStorageError::MigrationChecksumMismatch { .. })
        ));
    }

    #[test]
    fn test_checksum_invalid_hex_hash() {
        let content = "CREATE TABLE users;";

        // Invalid hex (not enough characters)
        let result = verify_migration_checksum("V001__test.sql", content, "abc");
        assert!(matches!(
            result,
            Err(AosStorageError::MigrationChecksumMismatch { .. })
        ));

        // Invalid hex (wrong characters)
        let result = verify_migration_checksum("V001__test.sql", content, &"gg".repeat(32));
        assert!(matches!(
            result,
            Err(AosStorageError::MigrationChecksumMismatch { .. })
        ));
    }

    #[test]
    fn test_checksum_case_sensitivity() {
        let content = "CREATE TABLE users;";
        let hash_lower = compute_hash(content);
        let hash_upper = hash_lower.to_uppercase();

        // Lowercase hash should work
        assert!(verify_migration_checksum("V001__test.sql", content, &hash_lower).is_ok());

        // Uppercase hash should NOT work (hex comparison is case-sensitive)
        let result = verify_migration_checksum("V001__test.sql", content, &hash_upper);
        assert!(matches!(
            result,
            Err(AosStorageError::MigrationChecksumMismatch { .. })
        ));
    }

    #[test]
    fn test_checksum_error_contains_filename() {
        let content = "CREATE TABLE test;";
        let wrong_hash = "0".repeat(64);

        match verify_migration_checksum("V999__special_file.sql", content, &wrong_hash) {
            Err(AosStorageError::MigrationChecksumMismatch {
                filename,
                expected,
                actual,
            }) => {
                assert_eq!(filename, "V999__special_file.sql");
                assert_eq!(expected, wrong_hash);
                assert_eq!(actual, compute_hash(content));
            }
            _ => panic!("Expected MigrationChecksumMismatch error"),
        }
    }

    #[test]
    fn test_checksum_multiline_sql() {
        let content = r#"
-- Migration: Create users table
-- Author: test
-- Date: 2024-01-01

CREATE TABLE users (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    username TEXT NOT NULL UNIQUE,
    email TEXT NOT NULL UNIQUE,
    created_at TIMESTAMP DEFAULT CURRENT_TIMESTAMP
);

CREATE INDEX idx_users_email ON users(email);
CREATE INDEX idx_users_username ON users(username);

-- Insert default admin user
INSERT INTO users (username, email) VALUES ('admin', 'admin@example.com');
"#;
        let hash = compute_hash(content);
        assert!(verify_migration_checksum("V001__create_users.sql", content, &hash).is_ok());
    }

    #[test]
    fn test_checksum_special_sql_characters() {
        let content = r#"
CREATE TABLE test (
    data TEXT CHECK (data LIKE '%''%'),
    json_col TEXT DEFAULT '{"key": "value"}',
    regex_col TEXT CHECK (regex_col ~ '^[a-z]+$')
);
INSERT INTO test VALUES ('it''s a test', '{}', 'abc');
"#;
        let hash = compute_hash(content);
        assert!(verify_migration_checksum("V001__special.sql", content, &hash).is_ok());
    }

    // =========================================================================
    // VersionCheck enum tests
    // =========================================================================

    #[test]
    fn test_version_check_debug() {
        let check = VersionCheck::NeedsMigration { from: 1, to: 5 };
        let debug_str = format!("{:?}", check);
        assert!(debug_str.contains("NeedsMigration"));
        assert!(debug_str.contains("1"));
        assert!(debug_str.contains("5"));
    }

    #[test]
    fn test_version_check_clone() {
        let original = VersionCheck::DatabaseAhead {
            app_version: 3,
            db_version: 7,
        };
        let cloned = original.clone();
        assert_eq!(original, cloned);
    }

    #[test]
    fn test_version_check_equality() {
        let a = VersionCheck::Compatible;
        let b = VersionCheck::Compatible;
        assert_eq!(a, b);

        let c = VersionCheck::NeedsMigration { from: 1, to: 2 };
        let d = VersionCheck::NeedsMigration { from: 1, to: 2 };
        assert_eq!(c, d);

        let e = VersionCheck::NeedsMigration { from: 1, to: 3 };
        assert_ne!(c, e);
    }

    // =========================================================================
    // AppliedMigration struct tests
    // =========================================================================

    #[test]
    fn test_applied_migration_debug() {
        let migration = AppliedMigration {
            filename: "V001__test.sql".into(),
            version: 1,
            applied_at: 1704067200,
        };
        let debug_str = format!("{:?}", migration);
        assert!(debug_str.contains("V001__test.sql"));
        assert!(debug_str.contains("1704067200"));
    }

    #[test]
    fn test_applied_migration_clone() {
        let original = AppliedMigration {
            filename: "V001__test.sql".into(),
            version: 1,
            applied_at: 1000,
        };
        let cloned = original.clone();
        assert_eq!(original.filename, cloned.filename);
        assert_eq!(original.version, cloned.version);
        assert_eq!(original.applied_at, cloned.applied_at);
    }
}
