//! Key lifecycle tracking and age monitoring

use adapteros_db::Db;
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
                    let created_at =
                        self.get_keychain_creation_date(key_label)
                            .unwrap_or_else(|| {
                                tracing::warn!(
                                "Could not get keychain creation date for {}, using current time",
                                key_label
                            );
                                std::time::SystemTime::now()
                                    .duration_since(std::time::UNIX_EPOCH)
                                    .expect("System time before UNIX epoch")
                                    .as_secs() as i64
                            });

                    let source = if created_at
                        == std::time::SystemTime::now()
                            .duration_since(std::time::UNIX_EPOCH)
                            .expect("System time before UNIX epoch")
                            .as_secs() as i64
                    {
                        "manual"
                    } else {
                        "keychain"
                    };

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
    fn get_keychain_creation_date(&self, _key_label: &str) -> Option<i64> {
        // TODO: Implement macOS keychain metadata extraction
        // This would use Security Framework to query kSecAttrCreationDate
        // For now, return None to fall back to manual tracking

        // Example implementation (requires additional Security Framework bindings):
        // use security_framework::item::{ItemSearchOptions, ItemClass};
        // let mut search = ItemSearchOptions::new();
        // search.class(ItemClass::key());
        // search.label(key_label);
        // search.load_attributes(true);
        // if let Ok(results) = search.search() {
        //     // Extract kSecAttrCreationDate from results
        // }

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
                        let age_days = (std::time::SystemTime::now()
                            .duration_since(std::time::UNIX_EPOCH)
                            .expect("System time before UNIX epoch")
                            .as_secs() as i64
                            - key.created_at)
                            / 86400;

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

                // TODO: Emit telemetry events for each warning
                // This will be integrated when telemetry is wired up
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

                let now = std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .expect("System time before UNIX epoch")
                    .as_secs() as i64;

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
