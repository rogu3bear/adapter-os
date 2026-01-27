//! SQLite persistence for CodeGraph
//!
//! Provides deterministic storage and retrieval of code graphs
//! using SQLite with proper schema and indexing.

use crate::types::Span;
use crate::{
    CallGraph, CodeGraph, Language, SymbolId, SymbolKind, SymbolNode, TypeAnnotation, Visibility,
};
use adapteros_core::{AosError, Result};
use serde_json;
use sqlx::{sqlite::SqliteConnectOptions, Row, SqlitePool};
use std::collections::BTreeMap;
use std::path::Path;

/// Database configuration
#[derive(Debug, Clone)]
pub struct DbConfig {
    /// Database path
    pub path: String,
    /// Connection pool size
    pub pool_size: u32,
    /// Enable WAL mode
    pub enable_wal: bool,
}

impl Default for DbConfig {
    fn default() -> Self {
        Self {
            path: "codegraph.db".to_string(),
            pool_size: 10,
            enable_wal: true,
        }
    }
}

/// SQLite database wrapper for CodeGraph
pub struct CodeGraphDb {
    /// Database connection pool
    pool: SqlitePool,
    /// Database path
    path: String,
}

impl CodeGraphDb {
    /// Create a new database connection
    pub async fn new<P: AsRef<Path>>(path: P) -> Result<Self> {
        let path_ref = path.as_ref();
        let path_str = path_ref.to_string_lossy().to_string();
        let options = SqliteConnectOptions::new()
            .filename(path_ref)
            .create_if_missing(true);

        let pool = SqlitePool::connect_with(options)
            .await
            .map_err(|e| AosError::Database(format!("Failed to connect to database: {}", e)))?;

        let db = Self {
            pool,
            path: path_str,
        };

        // Initialize schema
        db.init_schema().await?;

        Ok(db)
    }

    /// Initialize database schema
    async fn init_schema(&self) -> Result<()> {
        // Create symbols table
        sqlx::query(
            r#"
            CREATE TABLE IF NOT EXISTS symbols (
                id TEXT PRIMARY KEY,
                name TEXT NOT NULL,
                kind TEXT NOT NULL,
                language TEXT NOT NULL DEFAULT 'rust',
                type_annotation TEXT,
                signature TEXT,
                docstring TEXT,
                span_start_line INTEGER NOT NULL,
                span_start_column INTEGER NOT NULL,
                span_end_line INTEGER NOT NULL,
                span_end_column INTEGER NOT NULL,
                span_byte_start INTEGER NOT NULL,
                span_byte_length INTEGER NOT NULL,
                visibility TEXT NOT NULL,
                file_path TEXT NOT NULL,
                module_path TEXT,
                is_recursive BOOLEAN NOT NULL DEFAULT 0,
                is_async BOOLEAN NOT NULL DEFAULT 0,
                is_unsafe BOOLEAN NOT NULL DEFAULT 0,
                created_at INTEGER NOT NULL DEFAULT (strftime('%s', 'now'))
            )
        "#,
        )
        .execute(&self.pool)
        .await
        .map_err(|e| AosError::Database(format!("Failed to create symbols table: {}", e)))?;

        // Create call_edges table
        sqlx::query(
            r#"
            CREATE TABLE IF NOT EXISTS call_edges (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                caller_id TEXT NOT NULL,
                callee_id TEXT NOT NULL,
                call_site TEXT NOT NULL,
                is_recursive BOOLEAN NOT NULL DEFAULT 0,
                is_trait_call BOOLEAN NOT NULL DEFAULT 0,
                is_generic_instantiation BOOLEAN NOT NULL DEFAULT 0,
                created_at INTEGER NOT NULL DEFAULT (strftime('%s', 'now')),
                FOREIGN KEY (caller_id) REFERENCES symbols (id),
                FOREIGN KEY (callee_id) REFERENCES symbols (id)
            )
        "#,
        )
        .execute(&self.pool)
        .await
        .map_err(|e| AosError::Database(format!("Failed to create call_edges table: {}", e)))?;

        // Create indexes for performance
        sqlx::query("CREATE INDEX IF NOT EXISTS idx_symbols_name ON symbols (name)")
            .execute(&self.pool)
            .await
            .map_err(|e| AosError::Database(format!("Failed to create name index: {}", e)))?;

        sqlx::query("CREATE INDEX IF NOT EXISTS idx_symbols_kind ON symbols (kind)")
            .execute(&self.pool)
            .await
            .map_err(|e| AosError::Database(format!("Failed to create kind index: {}", e)))?;

        sqlx::query("CREATE INDEX IF NOT EXISTS idx_symbols_file_path ON symbols (file_path)")
            .execute(&self.pool)
            .await
            .map_err(|e| AosError::Database(format!("Failed to create file_path index: {}", e)))?;

        sqlx::query("CREATE INDEX IF NOT EXISTS idx_call_edges_caller ON call_edges (caller_id)")
            .execute(&self.pool)
            .await
            .map_err(|e| AosError::Database(format!("Failed to create caller index: {}", e)))?;

        sqlx::query("CREATE INDEX IF NOT EXISTS idx_call_edges_callee ON call_edges (callee_id)")
            .execute(&self.pool)
            .await
            .map_err(|e| AosError::Database(format!("Failed to create callee index: {}", e)))?;

        // Create import_edges table
        sqlx::query(
            r#"
            CREATE TABLE IF NOT EXISTS import_edges (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                importer_id TEXT NOT NULL,
                imported_id TEXT NOT NULL,
                import_statement TEXT NOT NULL,
                source_language TEXT NOT NULL,
                target_language TEXT NOT NULL,
                created_at INTEGER NOT NULL DEFAULT (strftime('%s', 'now')),
                FOREIGN KEY (importer_id) REFERENCES symbols (id),
                FOREIGN KEY (imported_id) REFERENCES symbols (id)
            )
        "#,
        )
        .execute(&self.pool)
        .await
        .map_err(|e| AosError::Database(format!("Failed to create import_edges table: {}", e)))?;

        // Create indexes for import_edges
        sqlx::query(
            "CREATE INDEX IF NOT EXISTS idx_import_edges_importer ON import_edges (importer_id)",
        )
        .execute(&self.pool)
        .await
        .map_err(|e| AosError::Database(format!("Failed to create importer index: {}", e)))?;

        sqlx::query(
            "CREATE INDEX IF NOT EXISTS idx_import_edges_imported ON import_edges (imported_id)",
        )
        .execute(&self.pool)
        .await
        .map_err(|e| AosError::Database(format!("Failed to create imported index: {}", e)))?;

        sqlx::query("CREATE INDEX IF NOT EXISTS idx_import_edges_languages ON import_edges (source_language, target_language)")
            .execute(&self.pool)
            .await
            .map_err(|e| AosError::Database(format!("Failed to create languages index: {}", e)))?;

        // Create index for language column in symbols
        sqlx::query("CREATE INDEX IF NOT EXISTS idx_symbols_language ON symbols (language)")
            .execute(&self.pool)
            .await
            .map_err(|e| AosError::Database(format!("Failed to create language index: {}", e)))?;

        Ok(())
    }

    /// Save a CodeGraph to the database
    pub async fn save_codegraph(&self, codegraph: &CodeGraph) -> Result<()> {
        // Start transaction
        let mut tx = self
            .pool
            .begin()
            .await
            .map_err(|e| AosError::Database(format!("Failed to begin transaction: {}", e)))?;

        // Clear existing data
        sqlx::query("DELETE FROM import_edges")
            .execute(&mut *tx)
            .await
            .map_err(|e| AosError::Database(format!("Failed to clear import_edges: {}", e)))?;

        sqlx::query("DELETE FROM call_edges")
            .execute(&mut *tx)
            .await
            .map_err(|e| AosError::Database(format!("Failed to clear call_edges: {}", e)))?;

        sqlx::query("DELETE FROM symbols")
            .execute(&mut *tx)
            .await
            .map_err(|e| AosError::Database(format!("Failed to clear symbols: {}", e)))?;

        // Insert symbols
        for (id, symbol) in &codegraph.symbols {
            let type_annotation_json = symbol
                .type_annotation
                .as_ref()
                .and_then(|ta| serde_json::to_string(ta).ok());

            let module_path_json = if symbol.module_path.is_empty() {
                None
            } else {
                serde_json::to_string(&symbol.module_path).ok()
            };

            sqlx::query(
                r#"
                INSERT INTO symbols (
                    id, name, kind, language, type_annotation, signature, docstring,
                    span_start_line, span_start_column, span_end_line, span_end_column,
                    span_byte_start, span_byte_length, visibility, file_path, module_path,
                    is_recursive, is_async, is_unsafe
                ) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
            "#,
            )
            .bind(id.to_hex())
            .bind(&symbol.name)
            .bind(symbol.kind.to_string())
            .bind(symbol.language.to_string())
            .bind(type_annotation_json)
            .bind(&symbol.signature)
            .bind(&symbol.docstring)
            .bind(symbol.span.start_line)
            .bind(symbol.span.start_column)
            .bind(symbol.span.end_line)
            .bind(symbol.span.end_column)
            .bind(symbol.span.byte_start as i64)
            .bind(symbol.span.byte_length as i64)
            .bind(symbol.visibility.to_string())
            .bind(&symbol.file_path)
            .bind(module_path_json)
            .bind(symbol.is_recursive)
            .bind(symbol.is_async)
            .bind(symbol.is_unsafe)
            .execute(&mut *tx)
            .await
            .map_err(|e| AosError::Database(format!("Failed to insert symbol: {}", e)))?;
        }

        // Insert call edges
        for edge in &codegraph.call_graph.edges {
            sqlx::query(
                r#"
                INSERT INTO call_edges (
                    caller_id, callee_id, call_site, is_recursive,
                    is_trait_call, is_generic_instantiation
                ) VALUES (?, ?, ?, ?, ?, ?)
            "#,
            )
            .bind(edge.caller.to_hex())
            .bind(edge.callee.to_hex())
            .bind(&edge.call_site)
            .bind(edge.is_recursive)
            .bind(edge.is_trait_call)
            .bind(edge.is_generic_instantiation)
            .execute(&mut *tx)
            .await
            .map_err(|e| AosError::Database(format!("Failed to insert call edge: {}", e)))?;
        }

        // Insert import edges
        for edge in &codegraph.call_graph.import_edges {
            sqlx::query(
                r#"
                INSERT INTO import_edges (
                    importer_id, imported_id, import_statement,
                    source_language, target_language
                ) VALUES (?, ?, ?, ?, ?)
            "#,
            )
            .bind(edge.importer.to_hex())
            .bind(edge.imported.to_hex())
            .bind(&edge.import_statement)
            .bind(edge.source_language.to_string())
            .bind(edge.target_language.to_string())
            .execute(&mut *tx)
            .await
            .map_err(|e| AosError::Database(format!("Failed to insert import edge: {}", e)))?;
        }

        // Commit transaction
        tx.commit()
            .await
            .map_err(|e| AosError::Database(format!("Failed to commit transaction: {}", e)))?;

        Ok(())
    }

    /// Load a CodeGraph from the database
    pub async fn load_codegraph(&self) -> Result<CodeGraph> {
        // Load symbols
        let symbol_rows = sqlx::query("SELECT * FROM symbols")
            .fetch_all(&self.pool)
            .await
            .map_err(|e| AosError::Database(format!("Failed to load symbols: {}", e)))?;

        let mut symbols = BTreeMap::new();

        for row in symbol_rows {
            let id_hex: String = row.get("id");
            let id = SymbolId::from_hex(&id_hex)
                .map_err(|e| AosError::Database(format!("Invalid symbol ID: {}", e)))?;

            let name: String = row.get("name");
            let kind_str: String = row.get("kind");
            let kind = self.parse_symbol_kind(&kind_str)?;
            let language_str: String = row.get("language");
            let language = self.parse_language(&language_str)?;

            let span = Span::new(
                row.get("span_start_line"),
                row.get("span_start_column"),
                row.get("span_end_line"),
                row.get("span_end_column"),
                row.get::<i64, _>("span_byte_start") as usize,
                row.get::<i64, _>("span_byte_length") as usize,
            );

            let file_path: String = row.get("file_path");

            let mut symbol = SymbolNode::new(id.clone(), name, kind, language, span, file_path);

            // Set optional fields
            if let Some(type_annotation_json) = row.get::<Option<String>, _>("type_annotation") {
                if let Ok(type_annotation) =
                    serde_json::from_str::<TypeAnnotation>(&type_annotation_json)
                {
                    symbol.type_annotation = Some(type_annotation);
                }
            }

            if let Some(signature) = row.get::<Option<String>, _>("signature") {
                symbol.signature = Some(signature);
            }

            if let Some(docstring) = row.get::<Option<String>, _>("docstring") {
                symbol.docstring = Some(docstring);
            }

            let visibility_str: String = row.get("visibility");
            symbol.visibility = self.parse_visibility(&visibility_str)?;

            if let Some(module_path_json) = row.get::<Option<String>, _>("module_path") {
                if let Ok(module_path) = serde_json::from_str::<Vec<String>>(&module_path_json) {
                    symbol.module_path = module_path;
                }
            }

            symbol.is_recursive = row.get("is_recursive");
            symbol.is_async = row.get("is_async");
            symbol.is_unsafe = row.get("is_unsafe");

            symbols.insert(id, symbol);
        }

        // Load call edges
        let edge_rows = sqlx::query("SELECT * FROM call_edges")
            .fetch_all(&self.pool)
            .await
            .map_err(|e| AosError::Database(format!("Failed to load call edges: {}", e)))?;

        // Load import edges
        let import_edge_rows = sqlx::query("SELECT * FROM import_edges")
            .fetch_all(&self.pool)
            .await
            .map_err(|e| AosError::Database(format!("Failed to load import edges: {}", e)))?;

        let mut call_graph = CallGraph::new();

        for row in edge_rows {
            let caller_hex: String = row.get("caller_id");
            let callee_hex: String = row.get("callee_id");

            let caller = SymbolId::from_hex(&caller_hex)
                .map_err(|e| AosError::Database(format!("Invalid caller ID: {}", e)))?;
            let callee = SymbolId::from_hex(&callee_hex)
                .map_err(|e| AosError::Database(format!("Invalid callee ID: {}", e)))?;

            let edge = super::CallEdge {
                caller,
                callee,
                call_site: row.get("call_site"),
                is_recursive: row.get("is_recursive"),
                is_trait_call: row.get("is_trait_call"),
                is_generic_instantiation: row.get("is_generic_instantiation"),
            };

            call_graph.add_edge(edge);
        }

        for row in import_edge_rows {
            let importer_hex: String = row.get("importer_id");
            let imported_hex: String = row.get("imported_id");

            let importer = SymbolId::from_hex(&importer_hex)
                .map_err(|e| AosError::Database(format!("Invalid importer ID: {}", e)))?;
            let imported = SymbolId::from_hex(&imported_hex)
                .map_err(|e| AosError::Database(format!("Invalid imported ID: {}", e)))?;

            let source_language_str: String = row.get("source_language");
            let target_language_str: String = row.get("target_language");
            let source_language = self.parse_language(&source_language_str)?;
            let target_language = self.parse_language(&target_language_str)?;

            let edge = super::ImportEdge {
                importer,
                imported,
                import_statement: row.get("import_statement"),
                source_language,
                target_language,
            };

            call_graph.add_import_edge(edge);
        }

        // Compute content hash
        let content_hash = CodeGraph::compute_content_hash(&symbols, &call_graph);

        Ok(CodeGraph {
            symbols,
            call_graph,
            content_hash,
        })
    }

    /// Parse symbol kind from string
    fn parse_symbol_kind(&self, kind_str: &str) -> Result<SymbolKind> {
        match kind_str {
            "function" => Ok(SymbolKind::Function),
            "struct" => Ok(SymbolKind::Struct),
            "enum" => Ok(SymbolKind::Enum),
            "trait" => Ok(SymbolKind::Trait),
            "impl" => Ok(SymbolKind::Impl),
            "type" => Ok(SymbolKind::Type),
            "const" => Ok(SymbolKind::Const),
            "static" => Ok(SymbolKind::Static),
            "macro" => Ok(SymbolKind::Macro),
            "module" => Ok(SymbolKind::Module),
            "field" => Ok(SymbolKind::Field),
            "variant" => Ok(SymbolKind::Variant),
            "method" => Ok(SymbolKind::Method),
            "associated_type" => Ok(SymbolKind::AssociatedType),
            "associated_const" => Ok(SymbolKind::AssociatedConst),
            _ => Err(AosError::Database(format!(
                "Unknown symbol kind: {}",
                kind_str
            ))),
        }
    }

    /// Parse language from string
    fn parse_language(&self, language_str: &str) -> Result<Language> {
        match language_str {
            "rust" => Ok(Language::Rust),
            "python" => Ok(Language::Python),
            "typescript" => Ok(Language::TypeScript),
            "javascript" => Ok(Language::JavaScript),
            "go" => Ok(Language::Go),
            _ => Err(AosError::Database(format!(
                "Unknown language: {}",
                language_str
            ))),
        }
    }

    /// Parse visibility from string
    fn parse_visibility(&self, visibility_str: &str) -> Result<Visibility> {
        match visibility_str {
            "pub" => Ok(Visibility::Public),
            "private" => Ok(Visibility::Private),
            "pub(crate)" => Ok(Visibility::Crate),
            "pub(super)" => Ok(Visibility::Super),
            _ if visibility_str.starts_with("pub(in ") => {
                let path = visibility_str
                    .strip_prefix("pub(in ")
                    .and_then(|s| s.strip_suffix(")"))
                    .unwrap_or("");
                Ok(Visibility::InPath(path.to_string()))
            }
            _ => Err(AosError::Database(format!(
                "Unknown visibility: {}",
                visibility_str
            ))),
        }
    }

    /// Get database statistics
    pub async fn get_stats(&self) -> Result<DbStats> {
        let symbol_count: (i64,) = sqlx::query_as("SELECT COUNT(*) FROM symbols")
            .fetch_one(&self.pool)
            .await
            .map_err(|e| AosError::Database(format!("Failed to count symbols: {}", e)))?;

        let edge_count: (i64,) = sqlx::query_as("SELECT COUNT(*) FROM call_edges")
            .fetch_one(&self.pool)
            .await
            .map_err(|e| AosError::Database(format!("Failed to count edges: {}", e)))?;

        let import_edge_count: (i64,) = sqlx::query_as("SELECT COUNT(*) FROM import_edges")
            .fetch_one(&self.pool)
            .await
            .map_err(|e| AosError::Database(format!("Failed to count import edges: {}", e)))?;

        Ok(DbStats {
            symbol_count: symbol_count.0 as usize,
            edge_count: edge_count.0 as usize,
            import_edge_count: import_edge_count.0 as usize,
            database_path: self.path.clone(),
        })
    }
}

/// Database statistics
#[derive(Debug, Clone)]
pub struct DbStats {
    pub symbol_count: usize,
    pub edge_count: usize,
    pub import_edge_count: usize,
    pub database_path: String,
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    fn new_test_tempdir() -> TempDir {
        TempDir::with_prefix("aos-test-").expect("Test temp directory creation should succeed")
    }

    #[tokio::test]
    async fn test_database_creation() {
        let temp_dir = new_test_tempdir();
        let db_path = temp_dir.path().join("test.db");

        let _db = CodeGraphDb::new(&db_path)
            .await
            .expect("Database creation should succeed");
        assert!(db_path.exists());
    }

    #[tokio::test]
    async fn test_database_stats() {
        let temp_dir = new_test_tempdir();
        let db_path = temp_dir.path().join("test.db");

        let db = CodeGraphDb::new(&db_path)
            .await
            .expect("Database creation should succeed");
        let stats = db
            .get_stats()
            .await
            .expect("Getting database stats should succeed");

        assert_eq!(stats.symbol_count, 0);
        assert_eq!(stats.edge_count, 0);
    }
}
