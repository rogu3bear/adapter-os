//! Key lifecycle tracking and age monitoring

use adapteros_db::Db;
use std::time::{SystemTime, UNIX_EPOCH};

/// Get current unix timestamp safely, returning 0 on system time misconfiguration
fn current_unix_timestamp() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or_else(|_| {
            tracing::error!("System time before UNIX epoch - key age calculations will be incorrect");
            0
        })
}
#[cfg(all(target_os = "macos", feature = "secure-enclave"))]
use security_framework::item::{ItemClass, ItemSearchOptions, SearchResult};
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::Mutex;
use tokio::time;

/// Key lifecycle manager
pub struct KeyLifecycleManager {
    db: Arc<Mutex<Option<Db>>>,
    threshold_days: i64,
}

impl KeyLifecycleManager {
    /// Create a new key lifecycle manager
    pub fn new(db: Option<Db>, threshold_days: i64) -> Self {
        Self {
            db: Arc::new(Mutex::new(db)),
            threshold_days,
        }
    }

    /// Check and update metadata for a key
    pub async fn track_key(&self, key_label: &str, key_type: &str) {
        let db = self.db.lock().await;
        if let Some(db) = db.as_ref() {
            // Check if we already have metadata for this key
            match db.get_key_metadata(key_label).await {
                Ok(Some(_)) => {
                    // Key already tracked
                    tracing::debug!("Key {} already tracked", key_label);
                }
                Ok(None) => {
                    // New key - try to get creation date from keychain, else use now
                    let now = current_unix_timestamp();
                    let created_at =
                        self.get_keychain_creation_date(key_label)
                            .unwrap_or_else(|| {
                                tracing::warn!(
                                    "Could not get keychain creation date for {}, using current time",
                                    key_label
                                );
                                now
                            });

                    let source = if created_at == now { "manual" } else { "keychain" };

                    if let Err(e) = db
                        .upsert_key_metadata(key_label, created_at, source, key_type)
                        .await
                    {
                        tracing::error!("Failed to track key {}: {}", key_label, e);
                    } else {
                        tracing::info!("Started tracking key: {}", key_label);
                    }
                }
                Err(e) => {
                    tracing::error!("Failed to check key metadata: {}", e);
                }
            }
        }
    }

    /// Try to get key creation date from macOS keychain
    #[cfg(target_os = "macos")]
    fn get_keychain_creation_date(&self, _key_label: &str) -> Option<i64> {
        #[cfg(all(target_os = "macos", feature = "secure-enclave"))]
        {
            // Query Security.framework for key metadata including creation date
            let mut search = ItemSearchOptions::new();
            search.class(ItemClass::key());
            search.label(_key_label);
            search.load_attributes(true);

            match search.search() {
                Ok(results) => {
                    // Check if we got any results - indicates key exists
                    let results_vec: Vec<SearchResult> = results.into_iter().collect();
                    if !results_vec.is_empty() {
                        // The security-framework crate's SearchResult::Dict variant provides
                        // access to keychain item attributes. However, extracting the creation
                        // date requires low-level CFDictionary operations that are platform-specific.
                        // For now, we log that we found the key but couldn't extract the date.
                        //
                        // Full implementation would require:
                        // 1. Using kSecAttrCreationDate constant from Security.framework
                        // 2. Converting CFDate to Unix timestamp (CFAbsoluteTime + 978307200)
                        tracing::debug!(
                            key_label,
                            result_count = results.len(),
                            "Key found in keychain - creation date extraction not yet implemented"
                        );
                    }
                    None
                }
                Err(e) => {
                    tracing::debug!(
                        key_label = _key_label,
                        error = %e,
                        "Failed to query keychain for key metadata"
                    );
                    None
                }
            }
        }
        #[cfg(not(all(target_os = "macos", feature = "secure-enclave")))]
        {
            None
        }
    }

    /// Try to get key creation date from macOS keychain (non-macOS fallback)
    #[cfg(not(target_os = "macos"))]
    fn get_keychain_creation_date(&self, _key_label: &str) -> Option<i64> {
        // Keychain metadata extraction only available on macOS
        None
    }

    /// Check all keys for age warnings
    pub async fn check_key_ages(&self) -> Vec<KeyAgeWarning> {
        let mut warnings = Vec::new();

        let db = self.db.lock().await;
        if let Some(db) = db.as_ref() {
            match db.list_old_keys(self.threshold_days).await {
                Ok(old_keys) => {
                    for key in old_keys {
                        let age_days = (current_unix_timestamp() - key.created_at) / 86400;

                        let key_label = key.key_label.clone();
                        warnings.push(KeyAgeWarning {
                            key_label: key_label.clone(),
                            age_days,
                            threshold_days: self.threshold_days,
                        });

                        tracing::warn!(
                            "Key {} is {} days old (threshold: {})",
                            key_label,
                            age_days,
                            self.threshold_days
                        );
                    }
                }
                Err(e) => {
                    tracing::error!("Failed to check key ages: {}", e);
                }
            }
        }

        warnings
    }

    /// Spawn a background task that checks key ages periodically
    pub async fn spawn_age_checker(self: Arc<Self>, check_interval: Duration) {
        let mut interval_timer = time::interval(check_interval);

        loop {
            interval_timer.tick().await;

            let warnings = self.check_key_ages().await;
            if !warnings.is_empty() {
                tracing::warn!("Found {} key age warnings", warnings.len());

                // Emit structured telemetry events for each warning
                for warning in &warnings {
                    tracing::warn!(
                        event = "key.age_warning",
                        key_label = %warning.key_label,
                        age_days = warning.age_days,
                        threshold_days = warning.threshold_days,
                        severity = if warning.age_days > warning.threshold_days * 2 { "critical" } else { "warning" },
                        "Key age exceeds threshold - rotation recommended"
                    );
                }
            } else {
                tracing::debug!("Key age check: all keys within threshold");
            }
        }
    }

    /// Get the maximum key age in days
    pub async fn get_max_key_age(&self) -> Option<i64> {
        let db = self.db.lock().await;
        if let Some(db) = db.as_ref() {
            if let Ok(keys) = db.list_all_keys().await {
                if keys.is_empty() {
                    return None;
                }

                let now = current_unix_timestamp();
                keys.into_iter().map(|k| (now - k.created_at) / 86400).max()
            } else {
                None
            }
        } else {
            None
        }
    }
}

/// Key age warning
#[derive(Debug, Clone)]
pub struct KeyAgeWarning {
    pub key_label: String,
    pub age_days: i64,
    pub threshold_days: i64,
}
