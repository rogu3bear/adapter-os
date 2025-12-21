//! Key-value backend trait and related types
//!
//! Provides a unified async trait for key-value storage operations, supporting
//! multiple backend implementations (SQLite, RocksDB, redb, etc.).
//!
//! # Design Principles
//!
//! - **Async-first**: All operations are async to support non-blocking I/O
//! - **Type-safe**: Strong typing with Result<T, StorageError> for error handling
//! - **Flexible**: Supports multiple query patterns (point, range, prefix)
//! - **Efficient**: Batch operations and transactions for optimal performance
//! - **Extensible**: Set operations enable secondary indexes and relationships

use async_trait::async_trait;

/// Storage-specific error type
///
/// Provides detailed error variants for different failure modes in storage operations.
#[derive(Debug, thiserror::Error)]
pub enum StorageError {
    /// Key not found in storage
    #[error("Key not found: {0}")]
    NotFound(String),

    /// Database operation failed
    #[error("Database error: {0}")]
    Database(String),

    /// Serialization or deserialization failed
    #[error("Serialization error: {0}")]
    Serialization(String),

    /// Transaction operation failed
    #[error("Transaction error: {0}")]
    Transaction(String),

    /// Invalid operation requested
    #[error("Invalid operation: {0}")]
    InvalidOperation(String),

    /// I/O error occurred
    #[error("IO error: {0}")]
    Io(String),

    /// Backend-specific error
    #[error("Backend error: {0}")]
    Backend(String),
}

impl From<std::io::Error> for StorageError {
    fn from(e: std::io::Error) -> Self {
        StorageError::Io(e.to_string())
    }
}

impl From<serde::de::value::Error> for StorageError {
    fn from(e: serde::de::value::Error) -> Self {
        StorageError::Serialization(e.to_string())
    }
}

impl From<bincode::Error> for StorageError {
    fn from(e: bincode::Error) -> Self {
        StorageError::Serialization(e.to_string())
    }
}

/// Key-value backend trait providing async storage operations
///
/// This trait defines a comprehensive interface for key-value storage backends,
/// supporting CRUD operations, batch processing, range queries, set operations,
/// and transactions. All implementations must be thread-safe (Send + Sync).
///
/// # Design Principles
///
/// - **Async-first**: All operations are async to support non-blocking I/O
/// - **Type-safe**: Strong typing with Result<T, StorageError> for error handling
/// - **Flexible**: Supports multiple query patterns (point, range, prefix)
/// - **Efficient**: Batch operations and transactions for optimal performance
/// - **Extensible**: Set operations enable secondary indexes and relationships
///
/// # Example
///
/// ```ignore
/// use adapteros_storage::backend::KvBackend;
///
/// async fn example(backend: &dyn KvBackend) -> Result<(), StorageError> {
///     // Store a value
///     backend.put("user:123", b"Alice").await?;
///
///     // Retrieve it
///     if let Some(data) = backend.get("user:123").await? {
///         println!("Found: {:?}", String::from_utf8_lossy(&data));
///     }
///
///     // Use batch operations for efficiency
///     let mut batch = backend.batch();
///     batch.put("user:124", b"Bob");
///     batch.put("user:125", b"Charlie");
///     batch.commit().await?;
///
///     Ok(())
/// }
/// ```
#[async_trait]
pub trait KvBackend: Send + Sync {
    // ========================================================================
    // Core CRUD Operations
    // ========================================================================

    /// Retrieves a value by key
    ///
    /// # Arguments
    ///
    /// * `key` - The key to look up
    ///
    /// # Returns
    ///
    /// * `Ok(Some(value))` - Key exists, returns the value as bytes
    /// * `Ok(None)` - Key does not exist
    /// * `Err(StorageError)` - Database or I/O error occurred
    ///
    /// # Example
    ///
    /// ```ignore
    /// if let Some(data) = backend.get("config:theme").await? {
    ///     let theme = String::from_utf8_lossy(&data);
    ///     println!("Theme: {}", theme);
    /// }
    /// ```
    async fn get(&self, key: &str) -> Result<Option<Vec<u8>>, StorageError>;

    /// Stores a key-value pair
    ///
    /// If the key already exists, its value is replaced atomically.
    ///
    /// # Arguments
    ///
    /// * `key` - The key to store
    /// * `value` - The value to associate with the key (as bytes)
    ///
    /// # Returns
    ///
    /// * `Ok(())` - Successfully stored
    /// * `Err(StorageError)` - Database or I/O error occurred
    ///
    /// # Example
    ///
    /// ```ignore
    /// backend.put("session:abc123", session_data.as_bytes()).await?;
    /// ```
    async fn put(&self, key: &str, value: &[u8]) -> Result<(), StorageError>;

    /// Deletes a key-value pair
    ///
    /// # Arguments
    ///
    /// * `key` - The key to delete
    ///
    /// # Returns
    ///
    /// * `Ok(true)` - Key existed and was deleted
    /// * `Ok(false)` - Key did not exist (no-op)
    /// * `Err(StorageError)` - Database or I/O error occurred
    ///
    /// # Example
    ///
    /// ```ignore
    /// if backend.delete("temp:processing").await? {
    ///     println!("Deleted temporary data");
    /// }
    /// ```
    async fn delete(&self, key: &str) -> Result<bool, StorageError>;

    /// Checks if a key exists without retrieving its value
    ///
    /// More efficient than `get()` when you only need to check existence.
    ///
    /// # Arguments
    ///
    /// * `key` - The key to check
    ///
    /// # Returns
    ///
    /// * `Ok(true)` - Key exists
    /// * `Ok(false)` - Key does not exist
    /// * `Err(StorageError)` - Database or I/O error occurred
    ///
    /// # Example
    ///
    /// ```ignore
    /// if backend.exists("lock:resource_123").await? {
    ///     return Err(StorageError::InvalidOperation("Resource locked".into()));
    /// }
    /// ```
    async fn exists(&self, key: &str) -> Result<bool, StorageError>;

    // ========================================================================
    // Batch Operations
    // ========================================================================

    /// Retrieves multiple values in a single operation
    ///
    /// More efficient than multiple individual `get()` calls. The returned vector
    /// has the same length and order as the input keys.
    ///
    /// # Arguments
    ///
    /// * `keys` - Slice of keys to retrieve
    ///
    /// # Returns
    ///
    /// * `Ok(values)` - Vector of Option<Vec<u8>>, one per input key
    /// * `Err(StorageError)` - Database or I/O error occurred
    ///
    /// # Example
    ///
    /// ```ignore
    /// let keys = vec!["user:1".to_string(), "user:2".to_string()];
    /// let values = backend.batch_get(&keys).await?;
    /// for (key, value) in keys.iter().zip(values.iter()) {
    ///     if let Some(data) = value {
    ///         println!("{}: {} bytes", key, data.len());
    ///     }
    /// }
    /// ```
    async fn batch_get(&self, keys: &[String]) -> Result<Vec<Option<Vec<u8>>>, StorageError>;

    /// Creates a new batch operation builder
    ///
    /// Batches allow multiple put/delete operations to be committed atomically
    /// and efficiently. Operations are not applied until `commit()` is called.
    ///
    /// # Returns
    ///
    /// A batch builder that can accumulate operations
    ///
    /// # Example
    ///
    /// ```ignore
    /// let mut batch = backend.batch();
    /// batch.put("counter:1", b"100");
    /// batch.put("counter:2", b"200");
    /// batch.delete("counter:old");
    /// batch.commit().await?;
    /// ```
    fn batch(&self) -> Box<dyn KvBatch + Send>;

    // ========================================================================
    // Range and Prefix Scans
    // ========================================================================

    /// Scans for all keys with a given prefix
    ///
    /// Returns up to `limit` key-value pairs where the key starts with `prefix`.
    /// Results are ordered lexicographically by key.
    ///
    /// # Arguments
    ///
    /// * `prefix` - The prefix to match (e.g., "user:" matches "user:123", "user:456")
    /// * `limit` - Maximum number of results to return
    ///
    /// # Returns
    ///
    /// * `Ok(results)` - Vector of (key, value) tuples
    /// * `Err(StorageError)` - Database or I/O error occurred
    ///
    /// # Example
    ///
    /// ```ignore
    /// // Get all sessions for a user
    /// let sessions = backend.scan_prefix("session:user123:", 100).await?;
    /// for (key, data) in sessions {
    ///     println!("Session: {}", key);
    /// }
    /// ```
    async fn scan_prefix(
        &self,
        prefix: &str,
        limit: usize,
    ) -> Result<Vec<(String, Vec<u8>)>, StorageError>;

    /// Scans for keys in a lexicographic range
    ///
    /// Returns up to `limit` key-value pairs where `start <= key < end`.
    /// Results are ordered lexicographically by key.
    ///
    /// # Arguments
    ///
    /// * `start` - Inclusive start of the range
    /// * `end` - Exclusive end of the range
    /// * `limit` - Maximum number of results to return
    ///
    /// # Returns
    ///
    /// * `Ok(results)` - Vector of (key, value) tuples
    /// * `Err(StorageError)` - Database or I/O error occurred
    ///
    /// # Example
    ///
    /// ```ignore
    /// // Get all users with IDs between 1000 and 2000
    /// let users = backend.scan_range("user:1000", "user:2000", 100).await?;
    /// ```
    async fn scan_range(
        &self,
        start: &str,
        end: &str,
        limit: usize,
    ) -> Result<Vec<(String, Vec<u8>)>, StorageError>;

    // ========================================================================
    // Set Operations (for Secondary Indexes)
    // ========================================================================

    /// Adds a member to a set
    ///
    /// Sets are useful for maintaining secondary indexes and relationships.
    /// Adding an existing member is idempotent (no error).
    ///
    /// # Arguments
    ///
    /// * `key` - The set identifier
    /// * `member` - The member to add
    ///
    /// # Example
    ///
    /// ```ignore
    /// // Index: adapter "model-v1" belongs to tenant "acme"
    /// backend.set_add("tenant:acme:adapters", "model-v1").await?;
    /// ```
    async fn set_add(&self, key: &str, member: &str) -> Result<(), StorageError>;

    /// Removes a member from a set
    ///
    /// Removing a non-existent member is idempotent (no error).
    ///
    /// # Arguments
    ///
    /// * `key` - The set identifier
    /// * `member` - The member to remove
    ///
    /// # Example
    ///
    /// ```ignore
    /// backend.set_remove("tenant:acme:adapters", "old-model").await?;
    /// ```
    async fn set_remove(&self, key: &str, member: &str) -> Result<(), StorageError>;

    /// Retrieves all members of a set
    ///
    /// # Arguments
    ///
    /// * `key` - The set identifier
    ///
    /// # Returns
    ///
    /// * `Ok(members)` - Vector of member strings
    /// * `Err(StorageError)` - Database or I/O error occurred
    ///
    /// # Example
    ///
    /// ```ignore
    /// let adapters = backend.set_members("tenant:acme:adapters").await?;
    /// println!("Tenant has {} adapters", adapters.len());
    /// ```
    async fn set_members(&self, key: &str) -> Result<Vec<String>, StorageError>;

    /// Checks if a member exists in a set
    ///
    /// More efficient than retrieving all members when checking a single value.
    ///
    /// # Arguments
    ///
    /// * `key` - The set identifier
    /// * `member` - The member to check
    ///
    /// # Returns
    ///
    /// * `Ok(true)` - Member exists in the set
    /// * `Ok(false)` - Member does not exist
    /// * `Err(StorageError)` - Database or I/O error occurred
    ///
    /// # Example
    ///
    /// ```ignore
    /// if backend.set_is_member("active:workers", "worker-123").await? {
    ///     println!("Worker is active");
    /// }
    /// ```
    async fn set_is_member(&self, key: &str, member: &str) -> Result<bool, StorageError>;

    // ========================================================================
    // Transactions
    // ========================================================================

    /// Executes a function within a transaction
    ///
    /// All operations within the transaction function are applied atomically.
    /// If the function returns an error, the transaction is rolled back.
    ///
    /// # Type Parameters
    ///
    /// * `F` - Transaction function type
    /// * `R` - Return type of the transaction function
    ///
    /// # Arguments
    ///
    /// * `f` - Function that performs transactional operations
    ///
    /// # Returns
    ///
    /// * `Ok(result)` - Transaction committed successfully, returns function result
    /// * `Err(StorageError)` - Transaction failed and was rolled back
    ///
    /// # Example
    ///
    /// ```ignore
    /// // Transfer operation (atomic)
    /// backend.transaction(|txn| {
    ///     let balance = txn.get("account:123")?
    ///         .ok_or(StorageError::NotFound("account".into()))?;
    ///     let mut amount: i64 = bincode::deserialize(&balance)?;
    ///     amount -= 100;
    ///     txn.put("account:123", &bincode::serialize(&amount)?)?;
    ///     Ok(())
    /// }).await?;
    /// ```
    async fn transaction<F, R>(&self, f: F) -> Result<R, StorageError>
    where
        F: FnOnce(&dyn KvTransaction) -> Result<R, StorageError> + Send,
        R: Send;
}

/// Batch operation builder for accumulating multiple writes
///
/// Batches provide atomic and efficient execution of multiple operations.
/// All operations are buffered until `commit()` is called.
///
/// # Design
///
/// The batch accumulates operations in memory and applies them atomically
/// when committed. This is more efficient than individual operations and
/// provides transactional semantics.
pub trait KvBatch: Send {
    /// Adds a put operation to the batch
    ///
    /// # Arguments
    ///
    /// * `key` - The key to store
    /// * `value` - The value to associate with the key
    ///
    /// # Example
    ///
    /// ```ignore
    /// let mut batch = backend.batch();
    /// batch.put("user:1", b"Alice");
    /// batch.put("user:2", b"Bob");
    /// ```
    fn put(&mut self, key: &str, value: &[u8]);

    /// Adds a delete operation to the batch
    ///
    /// # Arguments
    ///
    /// * `key` - The key to delete
    ///
    /// # Example
    ///
    /// ```ignore
    /// let mut batch = backend.batch();
    /// batch.delete("temp:old");
    /// batch.delete("temp:expired");
    /// ```
    fn delete(&mut self, key: &str);

    /// Adds a set-add operation to the batch
    ///
    /// # Arguments
    ///
    /// * `key` - The set identifier
    /// * `member` - The member to add
    ///
    /// # Example
    ///
    /// ```ignore
    /// let mut batch = backend.batch();
    /// batch.set_add("tags:post:1", "rust");
    /// batch.set_add("tags:post:1", "async");
    /// ```
    fn set_add(&mut self, key: &str, member: &str);

    /// Adds a set-remove operation to the batch
    ///
    /// # Arguments
    ///
    /// * `key` - The set identifier
    /// * `member` - The member to remove
    ///
    /// # Example
    ///
    /// ```ignore
    /// let mut batch = backend.batch();
    /// batch.set_remove("tags:post:1", "draft");
    /// ```
    fn set_remove(&mut self, key: &str, member: &str);

    /// Commits all accumulated operations atomically
    ///
    /// All operations are applied in a single atomic transaction. If any
    /// operation fails, none of the operations are applied.
    ///
    /// # Returns
    ///
    /// * `Ok(())` - All operations committed successfully
    /// * `Err(StorageError)` - Batch failed, no operations were applied
    ///
    /// # Example
    ///
    /// ```ignore
    /// let mut batch = backend.batch();
    /// batch.put("counter:1", b"100");
    /// batch.put("counter:2", b"200");
    /// batch.commit().await?;
    /// ```
    async fn commit(self: Box<Self>) -> Result<(), StorageError>;
}

/// Transaction interface for atomic read-modify-write operations
///
/// Provides a synchronous interface for operations within a transaction.
/// All reads see a consistent snapshot, and all writes are applied atomically
/// when the transaction commits.
///
/// # Isolation
///
/// Transactions provide snapshot isolation - all reads within the transaction
/// see the database state as it was at transaction start, plus any modifications
/// made within the transaction.
pub trait KvTransaction: Send + Sync {
    /// Retrieves a value within the transaction
    ///
    /// Reads see a consistent snapshot of the database, including any
    /// modifications made earlier in this transaction.
    ///
    /// # Arguments
    ///
    /// * `key` - The key to look up
    ///
    /// # Returns
    ///
    /// * `Ok(Some(value))` - Key exists
    /// * `Ok(None)` - Key does not exist
    /// * `Err(StorageError)` - Error occurred
    ///
    /// # Example
    ///
    /// ```ignore
    /// backend.transaction(|txn| {
    ///     let value = txn.get("counter")?;
    ///     // Use value...
    ///     Ok(())
    /// }).await?;
    /// ```
    fn get(&self, key: &str) -> Result<Option<Vec<u8>>, StorageError>;

    /// Stores a key-value pair within the transaction
    ///
    /// The write is buffered and applied atomically when the transaction commits.
    ///
    /// # Arguments
    ///
    /// * `key` - The key to store
    /// * `value` - The value to associate with the key
    ///
    /// # Returns
    ///
    /// * `Ok(())` - Operation queued successfully
    /// * `Err(StorageError)` - Error occurred
    ///
    /// # Example
    ///
    /// ```ignore
    /// backend.transaction(|txn| {
    ///     txn.put("counter", b"42")?;
    ///     Ok(())
    /// }).await?;
    /// ```
    fn put(&self, key: &str, value: &[u8]) -> Result<(), StorageError>;

    /// Deletes a key-value pair within the transaction
    ///
    /// The deletion is buffered and applied atomically when the transaction commits.
    ///
    /// # Arguments
    ///
    /// * `key` - The key to delete
    ///
    /// # Returns
    ///
    /// * `Ok(true)` - Key existed and will be deleted
    /// * `Ok(false)` - Key did not exist
    /// * `Err(StorageError)` - Error occurred
    ///
    /// # Example
    ///
    /// ```ignore
    /// backend.transaction(|txn| {
    ///     if txn.delete("temp:lock")? {
    ///         println!("Released lock");
    ///     }
    ///     Ok(())
    /// }).await?;
    /// ```
    fn delete(&self, key: &str) -> Result<bool, StorageError>;
}
