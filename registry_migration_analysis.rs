//! Registry Migration Analysis Tool
//!
//! Analyzes old registry database structure and content to inform migration strategy.

use adapteros_core::{AosError, Result};
use rusqlite::{Connection, Row};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::Path;
use tracing::{info, warn};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SchemaAnalysis {
    pub tables: Vec<TableInfo>,
    pub data_patterns: DataPatterns,
    pub migration_risk: MigrationRisk,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TableInfo {
    pub name: String,
    pub columns: Vec<ColumnInfo>,
    pub row_count: i64,
    pub schema_sql: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ColumnInfo {
    pub name: String,
    pub data_type: String,
    pub nullable: bool,
    pub sample_values: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DataPatterns {
    pub adapter_id_patterns: Vec<String>,
    pub tenant_id_patterns: Vec<String>,
    pub hash_formats: Vec<String>,
    pub acl_patterns: Vec<String>,
    pub relationship_patterns: RelationshipPatterns,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RelationshipPatterns {
    pub adapters_per_tenant: HashMap<String, usize>,
    pub tenant_references: HashMap<String, Vec<String>>,
    pub hash_collisions: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum MigrationRisk {
    Low,
    Medium,
    High,
    Critical,
}

impl SchemaAnalysis {
    pub fn analyze(db_path: &Path) -> Result<Self> {
        info!("Analyzing registry database: {:?}", db_path);

        let conn = Connection::open(db_path)
            .map_err(|e| AosError::Database(format!("Failed to open database: {}", e)))?;

        let tables = Self::analyze_tables(&conn)?;
        let data_patterns = Self::analyze_data_patterns(&conn)?;
        let migration_risk = Self::assess_migration_risk(&tables, &data_patterns);

        Ok(SchemaAnalysis {
            tables,
            data_patterns,
            migration_risk,
        })
    }

    fn analyze_tables(conn: &Connection) -> Result<Vec<TableInfo>> {
        let mut tables = Vec::new();

        // Get all table names
        let mut stmt = conn.prepare("SELECT name FROM sqlite_master WHERE type='table' AND name NOT LIKE 'sqlite_%'")?;
        let table_names: Vec<String> = stmt.query_map([], |row| row.get(0))?
            .collect::<std::result::Result<_, _>>()?;

        for table_name in table_names {
            let table_info = Self::analyze_table(conn, &table_name)?;
            tables.push(table_info);
        }

        Ok(tables)
    }

    fn analyze_table(conn: &Connection, table_name: &str) -> Result<TableInfo> {
        // Get row count
        let row_count: i64 = conn.query_row(
            &format!("SELECT COUNT(*) FROM {}", table_name),
            [],
            |row| row.get(0),
        ).unwrap_or(0);

        // Get schema
        let schema_sql: Option<String> = conn.query_row(
            "SELECT sql FROM sqlite_master WHERE type='table' AND name = ?",
            [table_name],
            |row| row.get(0),
        ).ok();

        // Analyze columns
        let columns = Self::analyze_columns(conn, table_name)?;

        Ok(TableInfo {
            name: table_name.to_string(),
            columns,
            row_count,
            schema_sql,
        })
    }

    fn analyze_columns(conn: &Connection, table_name: &str) -> Result<Vec<ColumnInfo>> {
        let mut columns = Vec::new();

        // Get column information
        let mut stmt = conn.prepare(&format!("PRAGMA table_info({})", table_name))?;
        let column_rows = stmt.query_map([], |row| {
            Ok((
                row.get::<_, String>(1)?, // name
                row.get::<_, String>(2)?, // type
                row.get::<_, i32>(3)?,    // notnull
            ))
        })?.collect::<std::result::Result<Vec<_>, _>>()?;

        for (name, data_type, not_null) in column_rows {
            let sample_values = Self::sample_column_values(conn, table_name, &name)?;
            columns.push(ColumnInfo {
                name,
                data_type,
                nullable: not_null == 0,
                sample_values,
            });
        }

        Ok(columns)
    }

    fn sample_column_values(conn: &Connection, table_name: &str, column_name: &str) -> Result<Vec<String>> {
        let sql = format!("SELECT DISTINCT {} FROM {} LIMIT 10", column_name, table_name);
        let mut stmt = conn.prepare(&sql)?;
        let values: Vec<String> = stmt.query_map([], |row| {
            match row.get::<_, Option<String>>(0)? {
                Some(s) => Ok(s),
                None => Ok("<NULL>".to_string()),
            }
        })?.collect::<std::result::Result<_, _>>()?;

        Ok(values)
    }

    fn analyze_data_patterns(conn: &Connection) -> Result<DataPatterns> {
        let mut adapter_id_patterns = Vec::new();
        let mut tenant_id_patterns = Vec::new();
        let mut hash_formats = Vec::new();
        let mut acl_patterns = Vec::new();
        let mut relationship_patterns = RelationshipPatterns {
            adapters_per_tenant: HashMap::new(),
            tenant_references: HashMap::new(),
            hash_collisions: Vec::new(),
        };

        // Analyze adapters table if it exists
        if Self::table_exists(conn, "adapters")? {
            Self::analyze_adapter_patterns(conn, &mut adapter_id_patterns, &mut hash_formats, &mut acl_patterns, &mut relationship_patterns)?;
        }

        // Analyze tenants table if it exists
        if Self::table_exists(conn, "tenants")? {
            Self::analyze_tenant_patterns(conn, &mut tenant_id_patterns)?;
        }

        Ok(DataPatterns {
            adapter_id_patterns,
            tenant_id_patterns,
            hash_formats,
            acl_patterns,
            relationship_patterns,
        })
    }

    fn table_exists(conn: &Connection, table_name: &str) -> Result<bool> {
        let count: i64 = conn.query_row(
            "SELECT COUNT(*) FROM sqlite_master WHERE type='table' AND name = ?",
            [table_name],
            |row| row.get(0),
        )?;
        Ok(count > 0)
    }

    fn analyze_adapter_patterns(
        conn: &Connection,
        adapter_id_patterns: &mut Vec<String>,
        hash_formats: &mut Vec<String>,
        acl_patterns: &mut Vec<String>,
        relationship_patterns: &mut RelationshipPatterns,
    ) -> Result<()> {
        let sql = "SELECT id, hash, acl FROM adapters LIMIT 1000";
        if let Ok(mut stmt) = conn.prepare(sql) {
            let rows = stmt.query_map([], |row| {
                Ok((
                    row.get::<_, String>(0)?, // id
                    row.get::<_, String>(1)?, // hash
                    row.get::<_, Option<String>>(2)?.unwrap_or_default(), // acl
                ))
            })?;

            for row_result in rows {
                if let Ok((id, hash, acl)) = row_result {
                    // Analyze ID patterns
                    Self::extract_id_patterns(&id, adapter_id_patterns);

                    // Analyze hash formats
                    Self::extract_hash_patterns(&hash, hash_formats);

                    // Analyze ACL patterns
                    Self::extract_acl_patterns(&acl, acl_patterns);

                    // Analyze relationships
                    Self::analyze_relationships(&id, &hash, relationship_patterns);
                }
            }
        }

        Ok(())
    }

    fn analyze_tenant_patterns(conn: &Connection, tenant_id_patterns: &mut Vec<String>) -> Result<()> {
        let sql = "SELECT id FROM tenants LIMIT 1000";
        if let Ok(mut stmt) = conn.prepare(sql) {
            let rows = stmt.query_map([], |row| {
                row.get::<_, String>(0)
            })?;

            for row_result in rows {
                if let Ok(id) = row_result {
                    Self::extract_id_patterns(&id, tenant_id_patterns);
                }
            }
        }

        Ok(())
    }

    fn extract_id_patterns(id: &str, patterns: &mut Vec<String>) {
        // Extract patterns like "tenant-adapter" structure
        if id.contains('-') {
            let parts: Vec<&str> = id.split('-').collect();
            if parts.len() >= 2 {
                patterns.push(format!("{}-*", parts[0]));
            }
        } else {
            patterns.push("single-part".to_string());
        }
    }

    fn extract_hash_patterns(hash: &str, patterns: &mut Vec<String>) {
        // Analyze hash format and length
        let len = hash.len();
        if hash.chars().all(|c| c.is_ascii_hexdigit()) {
            patterns.push(format!("hex-{}", len));
        } else {
            patterns.push(format!("other-{}", len));
        }
    }

    fn extract_acl_patterns(acl: &str, patterns: &mut Vec<String>) {
        if acl.trim().is_empty() {
            patterns.push("empty".to_string());
        } else if acl.contains(',') {
            patterns.push("comma-separated".to_string());
        } else {
            patterns.push("single-value".to_string());
        }
    }

    fn analyze_relationships(
        id: &str,
        hash: &str,
        patterns: &mut RelationshipPatterns,
    ) {
        // Extract tenant from adapter ID
        if let Some(tenant_id) = id.split('-').next() {
            *patterns.adapters_per_tenant.entry(tenant_id.to_string()).or_insert(0) += 1;

            patterns.tenant_references.entry(tenant_id.to_string())
                .or_insert_with(Vec::new)
                .push(id.to_string());
        }

        // Check for hash collisions (same hash, different IDs)
        // This is a simplified check - in practice you'd want to track all hashes
        if patterns.hash_collisions.len() < 10 { // Limit for analysis
            patterns.hash_collisions.push(format!("{}:{}", hash, id));
        }
    }

    fn assess_migration_risk(tables: &[TableInfo], patterns: &DataPatterns) -> MigrationRisk {
        let mut risk_score = 0;

        // Risk factors
        if tables.is_empty() {
            return MigrationRisk::Low; // Nothing to migrate
        }

        // Unknown ID patterns increase risk
        if patterns.adapter_id_patterns.contains(&"single-part".to_string()) {
            risk_score += 2; // Hard to extract tenant/name
        }

        // Hash format issues
        if patterns.hash_formats.contains(&"other-64".to_string()) {
            risk_score += 1; // May not be B3Hash compatible
        }

        // Complex ACL patterns
        if patterns.acl_patterns.contains(&"comma-separated".to_string()) {
            risk_score += 1; // Need careful JSON transformation
        }

        // Relationship complexity
        if patterns.relationship_patterns.adapters_per_tenant.len() > 10 {
            risk_score += 1; // Many tenants to validate
        }

        match risk_score {
            0..=1 => MigrationRisk::Low,
            2..=3 => MigrationRisk::Medium,
            4..=5 => MigrationRisk::High,
            _ => MigrationRisk::Critical,
        }
    }
}

impl std::fmt::Display for SchemaAnalysis {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        writeln!(f, "Registry Database Analysis")?;
        writeln!(f, "=========================")?;
        writeln!(f, "Migration Risk: {:?}", self.migration_risk)?;
        writeln!(f)?;

        writeln!(f, "Tables:")?;
        for table in &self.tables {
            writeln!(f, "  - {}: {} rows", table.name, table.row_count)?;
            for col in &table.columns {
                writeln!(f, "    * {} ({}) - {} values",
                    col.name, col.data_type,
                    col.sample_values.len())?;
            }
        }
        writeln!(f)?;

        writeln!(f, "Data Patterns:")?;
        writeln!(f, "  Adapter ID patterns: {:?}", self.data_patterns.adapter_id_patterns)?;
        writeln!(f, "  Tenant ID patterns: {:?}", self.data_patterns.tenant_id_patterns)?;
        writeln!(f, "  Hash formats: {:?}", self.data_patterns.hash_formats)?;
        writeln!(f, "  ACL patterns: {:?}", self.data_patterns.acl_patterns)?;
        writeln!(f, "  Adapters per tenant: {}", self.data_patterns.relationship_patterns.adapters_per_tenant.len())?;

        Ok(())
    }
}
