use crate::{Db, KvDb, Result, StorageMode};
use adapteros_core::AosError;
use std::marker::PhantomData;
use std::ops::Deref;
use std::sync::Arc;

/// Wrapper that restricts write-only adapter state mutations to callers that hold a lifecycle token.
#[derive(Clone)]
pub struct ProtectedDb {
    inner: Arc<Db>,
}

impl ProtectedDb {
    /// Create a new protected handle around the underlying database.
    pub fn new(db: Db) -> Self {
        Self {
            inner: Arc::new(db),
        }
    }

    /// Construct from an existing shared database handle.
    pub fn from_arc(db: Arc<Db>) -> Self {
        Self { inner: db }
    }

    /// Borrow the underlying database for read-only access.
    pub fn raw(&self) -> &Db {
        &self.inner
    }

    /// Clone out an `Arc<Db>` for consumers that still need the raw database handle.
    pub fn as_db_arc(&self) -> Arc<Db> {
        self.inner.clone()
    }

    /// Issue a lifecycle token scoped to this database borrow.
    ///
    /// Callers are expected to hold the appropriate lifecycle guard before
    /// minting a token; the type system forces the token to be passed explicitly.
    pub fn lifecycle_token(&self) -> LifecycleToken<'_> {
        LifecycleToken {
            _marker: PhantomData,
        }
    }

    /// Unlock a write-capable view using a lifecycle token.
    pub fn write<'a>(&'a self, token: LifecycleToken<'a>) -> WriteCapableDb<'a> {
        WriteCapableDb {
            db: &self.inner,
            _token: token,
        }
    }

    /// Convenience helper to unlock lifecycle-scoped writes without threading the token explicitly.
    pub fn write_guard(&self) -> WriteCapableDb<'_> {
        self.write(self.lifecycle_token())
    }

    /// Mutate the storage mode when this handle is uniquely owned.
    pub fn set_storage_mode(&mut self, mode: StorageMode) -> Result<()> {
        self.with_db_mut(|db| db.set_storage_mode(mode))
    }

    /// Attach a KV backend when this handle is uniquely owned.
    pub fn attach_kv_backend(&mut self, kv: KvDb) -> Result<()> {
        self.with_db_mut(|db| {
            db.attach_kv_backend(kv);
            Ok(())
        })
    }

    /// Detach the KV backend when this handle is uniquely owned.
    pub fn detach_kv_backend(&mut self) -> Result<()> {
        self.with_db_mut(|db| {
            db.detach_kv_backend();
            Ok(())
        })
    }

    fn with_db_mut<T>(&mut self, f: impl FnOnce(&mut Db) -> Result<T>) -> Result<T> {
        Arc::get_mut(&mut self.inner).map(f).unwrap_or_else(|| {
            Err(AosError::database(
                "ProtectedDb mutation requires an unshared handle".to_string(),
            ))
        })
    }
}

impl Deref for ProtectedDb {
    type Target = Db;

    fn deref(&self) -> &Self::Target {
        &self.inner
    }
}

/// Token that must be provided to perform adapter lifecycle mutations.
#[derive(Clone, Copy, Debug)]
pub struct LifecycleToken<'a> {
    _marker: PhantomData<&'a ()>,
}

/// Database view that exposes lifecycle mutations.
pub struct WriteCapableDb<'a> {
    pub(crate) db: &'a Db,
    _token: LifecycleToken<'a>,
}

impl<'a> Deref for WriteCapableDb<'a> {
    type Target = Db;

    fn deref(&self) -> &Self::Target {
        self.db
    }
}
