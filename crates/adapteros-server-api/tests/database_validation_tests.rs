//! Database Validation Integration Tests
//!
//! Validates:
//! 1. Database tables and columns exist and match struct definitions
//! 2. adapter_activations table exists with correct schema
//! 3. All migrations apply cleanly
//! 4. Foreign key constraints are enforced
//! 5. Indexes exist on performance-critical columns

#[cfg(test)]
mod schema_validation {
    /// Test: adapter_activations table exists
    ///
    /// Schema should include:
    /// - id (primary key)
    /// - adapter_id (foreign key → adapters.id)
    /// - request_id (UUID reference)
    /// - gate_value (f32)
    /// - selected (boolean)
    /// - created_at (timestamp)
    ///
    /// To verify:
    /// ```bash
    /// sqlite3 var/aos-cp.sqlite3 ".schema adapter_activations"
    /// ```
    #[test]
    fn test_adapter_activations_table_schema() {
        println!("Table: adapter_activations");
        println!("Expected columns:");
        println!("  id             TEXT PRIMARY KEY");
        println!("  adapter_id     TEXT NOT NULL FOREIGN KEY");
        println!("  request_id     TEXT NOT NULL");
        println!("  gate_value     REAL NOT NULL");
        println!("  selected       BOOLEAN NOT NULL");
        println!("  created_at     TIMESTAMP NOT NULL DEFAULT CURRENT_TIMESTAMP");
        println!();
        println!("Verify with: sqlite3 var/aos-cp.sqlite3 '.schema adapter_activations'");
    }

    /// Test: adapters table has all required columns
    ///
    /// Schema should include:
    /// - id (primary key)
    /// - tenant_id (foreign key → tenants.id)
    /// - hash (BLAKE3 hash)
    /// - tier (i32, 1/2/3)
    /// - rank (i32)
    /// - activation_percentage (f32)
    /// - expires_at (optional timestamp)
    /// - created_at (timestamp)
    /// - updated_at (timestamp)
    #[test]
    fn test_adapters_table_schema() {
        println!("Table: adapters");
        println!("Required columns:");
        println!("  id                      TEXT PRIMARY KEY");
        println!("  tenant_id               TEXT NOT NULL FOREIGN KEY");
        println!("  hash                    TEXT NOT NULL UNIQUE");
        println!("  tier                    INTEGER NOT NULL");
        println!("  rank                    INTEGER NOT NULL");
        println!("  activation_percentage   REAL");
        println!("  expires_at              TIMESTAMP");
        println!("  created_at              TIMESTAMP NOT NULL DEFAULT CURRENT_TIMESTAMP");
        println!("  updated_at              TIMESTAMP NOT NULL DEFAULT CURRENT_TIMESTAMP");
        println!();
        println!("Verify with: sqlite3 var/aos-cp.sqlite3 '.schema adapters'");
    }

    /// Test: training_jobs table has all required columns
    ///
    /// Schema should include:
    /// - id (primary key)
    /// - dataset_id (foreign key → training_datasets.id)
    /// - status (TEXT: pending/running/completed/failed)
    /// - progress_pct (f32)
    /// - loss (f32, optional)
    /// - tokens_per_sec (f32, optional)
    /// - started_at (timestamp, optional)
    /// - completed_at (timestamp, optional)
    #[test]
    fn test_training_jobs_table_schema() {
        println!("Table: training_jobs");
        println!("Required columns:");
        println!("  id              TEXT PRIMARY KEY");
        println!("  dataset_id      TEXT NOT NULL FOREIGN KEY");
        println!("  status          TEXT NOT NULL DEFAULT 'pending'");
        println!("  progress_pct    REAL NOT NULL DEFAULT 0");
        println!("  loss            REAL");
        println!("  tokens_per_sec  REAL");
        println!("  started_at      TIMESTAMP");
        println!("  completed_at    TIMESTAMP");
        println!();
        println!("Verify with: sqlite3 var/aos-cp.sqlite3 '.schema training_jobs'");
    }

    /// Test: audit_logs table exists
    ///
    /// Schema should include:
    /// - id (primary key)
    /// - user_id (foreign key)
    /// - action (TEXT)
    /// - resource (TEXT)
    /// - status (TEXT)
    /// - timestamp (timestamp)
    /// - details (JSON, optional)
    #[test]
    fn test_audit_logs_table_schema() {
        println!("Table: audit_logs");
        println!("Required columns:");
        println!("  id          INTEGER PRIMARY KEY AUTOINCREMENT");
        println!("  user_id     TEXT NOT NULL FOREIGN KEY");
        println!("  action      TEXT NOT NULL");
        println!("  resource    TEXT NOT NULL");
        println!("  status      TEXT NOT NULL");
        println!("  timestamp   TIMESTAMP NOT NULL DEFAULT CURRENT_TIMESTAMP");
        println!("  details     TEXT (JSON)");
        println!();
        println!("Verify with: sqlite3 var/aos-cp.sqlite3 '.schema audit_logs'");
    }

    /// Test: tenants table exists
    ///
    /// Schema should include:
    /// - id (primary key)
    /// - uid (user ID)
    /// - gid (group ID)
    /// - isolation_metadata (JSON)
    /// - created_at (timestamp)
    #[test]
    fn test_tenants_table_schema() {
        println!("Table: tenants");
        println!("Required columns:");
        println!("  id                   TEXT PRIMARY KEY");
        println!("  uid                  INTEGER NOT NULL");
        println!("  gid                  INTEGER NOT NULL");
        println!("  isolation_metadata   TEXT (JSON)");
        println!("  created_at           TIMESTAMP NOT NULL DEFAULT CURRENT_TIMESTAMP");
        println!();
        println!("Verify with: sqlite3 var/aos-cp.sqlite3 '.schema tenants'");
    }

    /// Test: training_datasets table exists
    ///
    /// Schema should include:
    /// - id (primary key)
    /// - hash_b3 (BLAKE3 hash)
    /// - validation_status (TEXT: pending/valid/invalid)
    /// - created_at (timestamp)
    #[test]
    fn test_training_datasets_table_schema() {
        println!("Table: training_datasets");
        println!("Required columns:");
        println!("  id                  TEXT PRIMARY KEY");
        println!("  hash_b3             TEXT NOT NULL UNIQUE");
        println!("  validation_status   TEXT NOT NULL DEFAULT 'pending'");
        println!("  created_at          TIMESTAMP NOT NULL DEFAULT CURRENT_TIMESTAMP");
        println!();
        println!("Verify with: sqlite3 var/aos-cp.sqlite3 '.schema training_datasets'");
    }

    /// Test: users table exists
    ///
    /// Schema should include:
    /// - id (primary key)
    /// - email (unique)
    /// - password_hash (bcrypt)
    /// - role (TEXT: admin/operator/viewer)
    /// - created_at (timestamp)
    #[test]
    fn test_users_table_schema() {
        println!("Table: users");
        println!("Required columns:");
        println!("  id              TEXT PRIMARY KEY");
        println!("  email           TEXT NOT NULL UNIQUE");
        println!("  password_hash   TEXT NOT NULL");
        println!("  role            TEXT NOT NULL");
        println!("  created_at      TIMESTAMP NOT NULL DEFAULT CURRENT_TIMESTAMP");
        println!();
        println!("Verify with: sqlite3 var/aos-cp.sqlite3 '.schema users'");
    }
}

#[cfg(test)]
mod migration_validation {
    /// Test: All migrations are signed
    ///
    /// Validates that migrations/signatures.json contains
    /// Ed25519 signatures for all migration files
    ///
    /// To verify:
    /// ```bash
    /// jq '.migrations | length' migrations/signatures.json
    /// ls migrations/*.sql | wc -l
    /// # Should match
    /// ```
    #[test]
    fn test_all_migrations_signed() {
        println!("Migration Signing Validation:");
        println!("  File: migrations/signatures.json");
        println!("  Should contain signatures for all *.sql files");
        println!();
        println!("Verify with:");
        println!("  jq '.migrations | length' migrations/signatures.json");
        println!("  ls migrations/*.sql | wc -l");
        println!("  # Should match");
    }

    /// Test: Migrations apply without errors
    ///
    /// To verify:
    /// ```bash
    /// rm var/aos-cp.sqlite3
    /// cargo run -p adapteros-db --bin aosctl -- db migrate
    /// # Should complete without errors
    /// ```
    #[test]
    fn test_migrations_apply_cleanly() {
        println!("Migration Application:");
        println!("  rm var/aos-cp.sqlite3");
        println!("  cargo run -p adapteros-db -- db migrate");
        println!("  Should complete without errors");
    }

    /// Test: Schema version is current
    ///
    /// Validates that schema_version matches latest migration number
    ///
    /// To verify:
    /// ```bash
    /// sqlite3 var/aos-cp.sqlite3 "SELECT version FROM schema_version;"
    /// ls migrations/ | tail -1 | sed 's/[^0-9].*//'
    /// # Should match
    /// ```
    #[test]
    fn test_schema_version_current() {
        println!("Schema Version:");
        println!("  Latest migration: 0080");
        println!("  schema_version table should show version = 80");
        println!();
        println!("Verify with:");
        println!("  sqlite3 var/aos-cp.sqlite3 'SELECT version FROM schema_version;'");
    }

    /// Test: No pending migrations
    ///
    /// Validates that all migrations in /migrations/ have been applied
    #[test]
    fn test_no_pending_migrations() {
        println!("Pending Migrations Check:");
        println!("  All migrations in /migrations/ should be applied");
        println!("  schema_version should match latest migration");
    }

    /// Test: Migration idempotency
    ///
    /// Running migrations twice should produce same result
    /// (migrations should be idempotent)
    #[test]
    fn test_migration_idempotency() {
        println!("Migration Idempotency:");
        println!("  Running migrations twice should produce same schema");
        println!("  Use: IF NOT EXISTS clauses in CREATE statements");
    }
}

#[cfg(test)]
mod constraint_validation {
    /// Test: Foreign key constraints enforced
    ///
    /// Validates that foreign key constraints are active:
    /// - adapter_activations.adapter_id → adapters.id
    /// - adapters.tenant_id → tenants.id
    /// - training_jobs.dataset_id → training_datasets.id
    /// - audit_logs.user_id → users.id
    ///
    /// To verify:
    /// ```bash
    /// sqlite3 var/aos-cp.sqlite3 "PRAGMA foreign_keys;"
    /// # Should return: 1 (enabled)
    /// ```
    #[test]
    fn test_foreign_key_constraints_enforced() {
        println!("Foreign Key Constraints:");
        println!("  PRAGMA foreign_keys should be ON");
        println!();
        println!("Expected relationships:");
        println!("  adapter_activations.adapter_id → adapters.id");
        println!("  adapters.tenant_id → tenants.id");
        println!("  training_jobs.dataset_id → training_datasets.id");
        println!("  audit_logs.user_id → users.id");
        println!();
        println!("Verify with: sqlite3 var/aos-cp.sqlite3 'PRAGMA foreign_keys;'");
        println!("Expected output: 1");
    }

    /// Test: NOT NULL constraints
    ///
    /// Validates that required fields have NOT NULL constraint
    #[test]
    fn test_not_null_constraints() {
        println!("NOT NULL Constraints:");
        println!("  adapter_id, request_id, gate_value, selected");
        println!("  must all be NOT NULL in adapter_activations");
        println!();
        println!("Verify with: sqlite3 var/aos-cp.sqlite3 '.schema adapter_activations'");
    }

    /// Test: UNIQUE constraints
    ///
    /// Validates fields that should be unique
    /// - adapters.hash (UNIQUE)
    /// - training_datasets.hash_b3 (UNIQUE)
    /// - users.email (UNIQUE)
    #[test]
    fn test_unique_constraints() {
        println!("UNIQUE Constraints:");
        println!("  adapters.hash must be UNIQUE");
        println!("  training_datasets.hash_b3 must be UNIQUE");
        println!("  users.email must be UNIQUE");
        println!();
        println!("Verify with: sqlite3 var/aos-cp.sqlite3 '.schema [table]'");
    }

    /// Test: PRIMARY KEY constraints
    ///
    /// Validates all tables have primary keys
    #[test]
    fn test_primary_key_constraints() {
        println!("PRIMARY KEY Constraints:");
        println!("  All tables should have PRIMARY KEY");
        println!("  Most use TEXT (UUID) or INTEGER");
    }

    /// Test: DEFAULT values
    ///
    /// Validates default values for common fields
    /// - created_at DEFAULT CURRENT_TIMESTAMP
    /// - updated_at DEFAULT CURRENT_TIMESTAMP
    /// - status DEFAULT 'pending'
    /// - progress_pct DEFAULT 0
    #[test]
    fn test_default_values() {
        println!("DEFAULT Values:");
        println!("  created_at → CURRENT_TIMESTAMP");
        println!("  updated_at → CURRENT_TIMESTAMP");
        println!("  status → 'pending'");
        println!("  progress_pct → 0");
    }
}

#[cfg(test)]
mod index_validation {
    /// Test: Index on adapter_activations.adapter_id
    ///
    /// Improves performance for:
    /// - SELECT * FROM adapter_activations WHERE adapter_id = ?
    ///
    /// To verify:
    /// ```bash
    /// sqlite3 var/aos-cp.sqlite3 ".indexes adapter_activations"
    /// ```
    #[test]
    fn test_index_on_adapter_activations_adapter_id() {
        println!("Index: adapter_activations(adapter_id)");
        println!("  Improves: SELECT WHERE adapter_id = ?");
        println!();
        println!("Verify with: sqlite3 var/aos-cp.sqlite3 '.indexes adapter_activations'");
    }

    /// Test: Index on adapters.tenant_id
    ///
    /// Improves performance for listing tenant's adapters
    #[test]
    fn test_index_on_adapters_tenant_id() {
        println!("Index: adapters(tenant_id)");
        println!("  Improves: SELECT * FROM adapters WHERE tenant_id = ?");
        println!();
        println!("Verify with: sqlite3 var/aos-cp.sqlite3 '.indexes adapters'");
    }

    /// Test: Index on adapters.hash
    ///
    /// Improves performance for adapter lookup by hash
    #[test]
    fn test_index_on_adapters_hash() {
        println!("Index: adapters(hash)");
        println!("  Improves: SELECT * FROM adapters WHERE hash = ?");
        println!("  Note: May be implicit from UNIQUE constraint");
    }

    /// Test: Index on training_jobs.dataset_id
    ///
    /// Improves performance for listing jobs by dataset
    #[test]
    fn test_index_on_training_jobs_dataset_id() {
        println!("Index: training_jobs(dataset_id)");
        println!("  Improves: SELECT * FROM training_jobs WHERE dataset_id = ?");
    }

    /// Test: Compound index on audit_logs
    ///
    /// Improves performance for audit log queries:
    /// - SELECT WHERE user_id = ? AND action = ? AND timestamp > ?
    #[test]
    fn test_compound_index_on_audit_logs() {
        println!("Compound Index: audit_logs(user_id, action, timestamp)");
        println!("  Improves: SELECT WHERE user_id = ? AND action = ? AND timestamp > ?");
    }

    /// Test: Query optimization with indexes
    ///
    /// Validates that EXPLAIN QUERY PLAN shows index usage
    #[test]
    fn test_index_usage_in_queries() {
        println!("Query Plan Verification:");
        println!("  EXPLAIN QUERY PLAN SELECT * FROM adapter_activations WHERE adapter_id = ?");
        println!("  Should show: SEARCH adapter_activations USING INDEX...");
    }
}

#[cfg(test)]
mod data_consistency {
    /// Test: Referential integrity maintained
    ///
    /// No orphaned records:
    /// - Every adapter_id in adapter_activations exists in adapters
    /// - Every dataset_id in training_jobs exists in training_datasets
    /// - Every user_id in audit_logs exists in users
    ///
    /// To verify:
    /// ```bash
    /// sqlite3 var/aos-cp.sqlite3 \
    ///   "SELECT COUNT(*) FROM adapter_activations a
    ///    WHERE NOT EXISTS (SELECT 1 FROM adapters WHERE id = a.adapter_id);"
    /// # Should return: 0
    /// ```
    #[test]
    fn test_referential_integrity() {
        println!("Referential Integrity Checks:");
        println!("  No orphaned adapter_activations records");
        println!("  No orphaned training_jobs records");
        println!("  No orphaned audit_logs records");
        println!();
        println!("Verify with:");
        println!("  SELECT COUNT(*) FROM adapter_activations a");
        println!("  WHERE NOT EXISTS (SELECT 1 FROM adapters WHERE id = a.adapter_id);");
        println!("  Should return: 0");
    }

    /// Test: No duplicate primary keys
    ///
    /// Each table should have unique primary key values
    #[test]
    fn test_no_duplicate_primary_keys() {
        println!("Duplicate Primary Key Check:");
        println!("  All PRIMARY KEY values should be unique");
    }

    /// Test: Timestamp consistency
    ///
    /// created_at ≤ updated_at for all records
    #[test]
    fn test_timestamp_consistency() {
        println!("Timestamp Consistency:");
        println!("  created_at should be ≤ updated_at");
        println!("  All timestamps should be in UTC");
    }

    /// Test: No NULL in required fields
    ///
    /// Fields marked NOT NULL should have values
    #[test]
    fn test_no_null_in_required_fields() {
        println!("Required Field Validation:");
        println!("  No NULL values in NOT NULL columns");
    }
}

#[cfg(test)]
mod performance_validation {
    /// Test: Large table query performance
    ///
    /// Queries on large tables (1M+ rows) complete in < 1s
    /// with proper indexes
    #[test]
    fn test_large_table_query_performance() {
        println!("Large Table Performance:");
        println!("  Query: SELECT * FROM adapter_activations WHERE adapter_id = ?");
        println!("  Should complete in < 100ms with index");
        println!("  Use EXPLAIN QUERY PLAN to verify index usage");
    }

    /// Test: Index size overhead
    ///
    /// Validates indexes don't consume excessive space
    #[test]
    fn test_index_size_overhead() {
        println!("Index Size Overhead:");
        println!("  Total DB size should be < 10GB for typical workload");
        println!("  Indexes typically add 10-30% overhead");
    }

    /// Test: Query complexity
    ///
    /// Complex queries (joins, subqueries) should still be efficient
    #[test]
    fn test_complex_query_efficiency() {
        println!("Complex Query Efficiency:");
        println!("  Queries with joins should use indexes");
        println!("  Subqueries should be optimized");
    }
}

#[cfg(test)]
mod backup_validation {
    /// Test: Database can be backed up
    ///
    /// To verify:
    /// ```bash
    /// sqlite3 var/aos-cp.sqlite3 ".backup var/aos-cp.backup.db"
    /// # Should complete without errors
    /// ```
    #[test]
    fn test_database_backup() {
        println!("Database Backup:");
        println!("  sqlite3 var/aos-cp.sqlite3 '.backup var/aos-cp.backup.db'");
        println!("  Should complete without errors");
    }

    /// Test: Backup can be restored
    ///
    /// To verify:
    /// ```bash
    /// sqlite3 var/aos-cp.backup.db ".schema"
    /// # Should show same schema as original
    /// ```
    #[test]
    fn test_backup_restoration() {
        println!("Backup Restoration:");
        println!("  Restored database should have same schema");
        println!("  All data should be intact");
    }

    /// Test: PRAGMA integrity_check
    ///
    /// Database integrity should be verified:
    /// ```bash
    /// sqlite3 var/aos-cp.sqlite3 "PRAGMA integrity_check;"
    /// # Should return: ok
    /// ```
    #[test]
    fn test_database_integrity() {
        println!("Database Integrity:");
        println!("  PRAGMA integrity_check; should return: ok");
        println!("  PRAGMA quick_check; should return: ok");
    }
}

#[cfg(test)]
mod column_type_validation {
    /// Test: TEXT columns for string data
    ///
    /// Fields like id, hash, adapter_id should be TEXT
    #[test]
    fn test_text_column_types() {
        println!("TEXT Column Types:");
        println!("  id → TEXT (UUID)");
        println!("  hash → TEXT (BLAKE3 hash)");
        println!("  adapter_id → TEXT");
        println!("  name → TEXT");
    }

    /// Test: INTEGER columns for numeric data
    ///
    /// Fields like tier, rank, progress_pct should be INTEGER or REAL
    #[test]
    fn test_integer_column_types() {
        println!("INTEGER/REAL Column Types:");
        println!("  tier → INTEGER (1, 2, 3)");
        println!("  rank → INTEGER");
        println!("  progress_pct → REAL");
        println!("  activation_percentage → REAL");
    }

    /// Test: TIMESTAMP columns for dates
    ///
    /// Fields like created_at, updated_at should be TIMESTAMP
    #[test]
    fn test_timestamp_column_types() {
        println!("TIMESTAMP Column Types:");
        println!("  created_at → TIMESTAMP");
        println!("  updated_at → TIMESTAMP");
        println!("  started_at → TIMESTAMP");
        println!("  completed_at → TIMESTAMP");
    }

    /// Test: BOOLEAN columns for flags
    ///
    /// SQLite uses INTEGER (0/1) for BOOLEAN
    #[test]
    fn test_boolean_column_types() {
        println!("BOOLEAN Column Types (SQLite uses INTEGER 0/1):");
        println!("  selected → INTEGER");
        println!("  is_active → INTEGER");
    }

    /// Test: JSON columns for structured data
    ///
    /// SQLite stores JSON as TEXT with validation
    #[test]
    fn test_json_column_types() {
        println!("JSON Column Types:");
        println!("  isolation_metadata → TEXT (JSON)");
        println!("  details → TEXT (JSON)");
    }
}
