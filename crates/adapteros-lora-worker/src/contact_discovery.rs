//! Contact Discovery Handler
//!
//! Implements contact discovery and tracking as part of the signal protocol.
//! Contacts are entities (users, adapters, repos) discovered during inference.
//!
//! Citation: CONTACTS_AND_STREAMS_IMPLEMENTATION_PLAN.md §2.4
//! Pattern from: crates/adapteros-lora-worker/src/signal_handlers.rs

use adapteros_core::{AosError, Result};
use adapteros_db::{contacts::ContactUpsertBuilder, Db};
use adapteros_telemetry::TelemetryWriter;
use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use serde_json::json;
use tracing::{debug, info, warn};

use crate::signal::{Signal, SignalHandler, SignalPriority, SignalType};

/// Contact category enumeration
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum ContactCategory {
    User,
    System,
    Adapter,
    Repository,
    External,
}

impl ContactCategory {
    pub fn parse(s: &str) -> Result<Self> {
        match s.to_lowercase().as_str() {
            "user" => Ok(ContactCategory::User),
            "system" => Ok(ContactCategory::System),
            "adapter" => Ok(ContactCategory::Adapter),
            "repository" => Ok(ContactCategory::Repository),
            "external" => Ok(ContactCategory::External),
            _ => Err(AosError::Worker(format!("Invalid contact category: {}", s))),
        }
    }

    pub fn as_str(&self) -> &'static str {
        match self {
            ContactCategory::User => "user",
            ContactCategory::System => "system",
            ContactCategory::Adapter => "adapter",
            ContactCategory::Repository => "repository",
            ContactCategory::External => "external",
        }
    }
}

impl std::str::FromStr for ContactCategory {
    type Err = AosError;

    fn from_str(s: &str) -> std::result::Result<Self, Self::Err> {
        Self::parse(s)
    }
}

/// Contact data structure for database operations
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Contact {
    pub name: String,
    pub email: Option<String>,
    pub category: ContactCategory,
    pub tenant_id: String,
    pub role: Option<String>,
    pub metadata: Option<serde_json::Value>,
    pub discovered_by: Option<String>,
}

/// Contact discovery event for telemetry
/// Citation: Pattern from crates/mplora-lifecycle/src/lib.rs lines 20-45
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ContactDiscoveryEvent {
    pub contact_name: String,
    pub category: String,
    pub tenant_id: String,
    pub trace_id: String,
    pub timestamp: u64,
}

/// Contact interaction event for telemetry
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ContactInteractionEvent {
    pub contact_name: String,
    pub interaction_type: String,
    pub trace_id: String,
    pub cpid: String,
    pub timestamp: u64,
}

/// Contact discovery handler
///
/// Processes ContactDiscovered, ContactUpdated, and ContactInteraction signals
/// and persists them to the database with telemetry logging.
pub struct ContactDiscoveryHandler {
    telemetry: TelemetryWriter,
    db: Option<Db>,
}

impl ContactDiscoveryHandler {
    /// Create a new contact discovery handler
    pub fn new(telemetry: TelemetryWriter) -> Self {
        Self {
            telemetry,
            db: None,
        }
    }

    /// Create a new contact discovery handler with database
    pub fn new_with_db(telemetry: TelemetryWriter, db: Db) -> Self {
        Self {
            telemetry,
            db: Some(db),
        }
    }

    /// Set database for persistence
    pub fn set_db(&mut self, db: Db) {
        self.db = Some(db);
    }

    /// Extract contact information from signal payload
    fn extract_contact(&self, signal: &Signal) -> Result<Option<Contact>> {
        let payload = &signal.payload;

        // Required fields
        let name = match payload.get("name").and_then(|v| v.as_str()) {
            Some(n) => n.to_string(),
            None => {
                warn!("ContactDiscovered signal missing 'name' field");
                return Ok(None);
            }
        };

        let category_str = match payload.get("category").and_then(|v| v.as_str()) {
            Some(c) => c,
            None => {
                warn!("ContactDiscovered signal missing 'category' field");
                return Ok(None);
            }
        };

        let category = ContactCategory::parse(category_str)?;

        let tenant_id = match payload.get("tenant_id").and_then(|v| v.as_str()) {
            Some(t) => t.to_string(),
            None => {
                warn!("ContactDiscovered signal missing 'tenant_id' field");
                return Ok(None);
            }
        };

        // Optional fields
        let email = payload
            .get("email")
            .and_then(|v| v.as_str())
            .map(|s| s.to_string());
        let role = payload
            .get("role")
            .and_then(|v| v.as_str())
            .map(|s| s.to_string());
        let metadata = payload.get("metadata").cloned();

        Ok(Some(Contact {
            name,
            email,
            category,
            tenant_id,
            role,
            metadata,
            discovered_by: signal.trace_id.clone(),
        }))
    }

    /// Upsert contact to database
    ///
    /// Citation: Database pattern from crates/mplora-db/src/contacts.rs
    async fn upsert_contact(&self, contact: &Contact) -> Result<()> {
        if let Some(ref db) = self.db {
            let metadata_json = contact
                .metadata
                .as_ref()
                .map(serde_json::to_string)
                .transpose()
                .map_err(|e| AosError::Worker(format!("Failed to serialize metadata: {}", e)))?;

            let params = ContactUpsertBuilder::new()
                .tenant_id(&contact.tenant_id)
                .name(&contact.name)
                .category(contact.category.as_str())
                .email(contact.email.clone())
                .role(contact.role.clone())
                .metadata_json(metadata_json)
                .discovered_by(contact.discovered_by.clone())
                .build()
                .map_err(|e| AosError::Worker(format!("Failed to build contact params: {}", e)))?;

            db.upsert_contact(params)
                .await
                .map_err(|e| AosError::Worker(format!("Failed to upsert contact: {}", e)))?;

            debug!("Contact upserted to database: {}", contact.name);
        } else {
            debug!("Contact upsert skipped (no database): {}", contact.name);
        }
        Ok(())
    }

    /// Log contact interaction to database
    async fn log_interaction(
        &self,
        tenant_id: &str,
        contact_name: &str,
        interaction_type: &str,
        trace_id: &str,
        cpid: &str,
        context: Option<&serde_json::Value>,
    ) -> Result<()> {
        if let Some(ref db) = self.db {
            db.log_contact_interaction(
                tenant_id,
                contact_name,
                trace_id,
                cpid,
                interaction_type,
                context,
            )
            .await
            .map_err(|e| AosError::Worker(format!("Failed to log contact interaction: {}", e)))?;

            debug!(
                "Contact interaction logged to database for: {}",
                contact_name
            );
        } else {
            debug!(
                "Contact interaction logging skipped (no database): {}",
                contact_name
            );
        }
        Ok(())
    }
}

#[async_trait]
impl SignalHandler for ContactDiscoveryHandler {
    fn signal_types(&self) -> Vec<SignalType> {
        vec![
            SignalType::ContactDiscovered,
            SignalType::ContactUpdated,
            SignalType::ContactInteraction,
        ]
    }

    async fn handle_signal(&mut self, signal: &Signal) -> Result<()> {
        match signal.signal_type {
            SignalType::ContactDiscovered => {
                if let Some(contact) = self.extract_contact(signal)? {
                    info!(
                        "Contact discovered: {} ({}) in tenant {}",
                        contact.name,
                        contact.category.as_str(),
                        contact.tenant_id
                    );

                    // Upsert to database
                    self.upsert_contact(&contact).await?;

                    // Log to telemetry
                    // Citation: Telemetry pattern from crates/mplora-lifecycle/src/lib.rs:265-272
                    let timestamp = std::time::SystemTime::now()
                        .duration_since(std::time::UNIX_EPOCH)
                        .expect("System time before UNIX epoch")
                        .as_secs();

                    self.telemetry.log(
                        "contact.discovered",
                        ContactDiscoveryEvent {
                            contact_name: contact.name.clone(),
                            category: contact.category.as_str().to_string(),
                            tenant_id: contact.tenant_id.clone(),
                            trace_id: signal.trace_id.clone().unwrap_or_default(),
                            timestamp,
                        },
                    )?;
                }
            }

            SignalType::ContactUpdated => {
                if let Some(contact) = self.extract_contact(signal)? {
                    info!(
                        "Contact updated: {} in tenant {}",
                        contact.name, contact.tenant_id
                    );

                    // Update in database
                    self.upsert_contact(&contact).await?;

                    // Log to telemetry
                    let timestamp = std::time::SystemTime::now()
                        .duration_since(std::time::UNIX_EPOCH)
                        .expect("System time before UNIX epoch")
                        .as_secs();

                    self.telemetry.log(
                        "contact.updated",
                        json!({
                            "contact_name": contact.name,
                            "category": contact.category.as_str(),
                            "tenant_id": contact.tenant_id,
                            "trace_id": signal.trace_id,
                            "timestamp": timestamp,
                        }),
                    )?;
                }
            }

            SignalType::ContactInteraction => {
                let payload = &signal.payload;

                let contact_name = match payload.get("contact_name").and_then(|v| v.as_str()) {
                    Some(n) => n,
                    None => {
                        warn!("ContactInteraction signal missing 'contact_name' field");
                        return Ok(());
                    }
                };

                let tenant_id = match payload.get("tenant_id").and_then(|v| v.as_str()) {
                    Some(t) => t,
                    None => {
                        warn!("ContactInteraction signal missing 'tenant_id' field");
                        return Ok(());
                    }
                };

                let interaction_type = payload
                    .get("interaction_type")
                    .and_then(|v| v.as_str())
                    .unwrap_or("mentioned");

                let trace_id = signal.trace_id.as_deref().unwrap_or("unknown");
                let cpid = payload
                    .get("cpid")
                    .and_then(|v| v.as_str())
                    .unwrap_or("unknown");

                debug!(
                    "Contact interaction: {} ({}) - trace: {}",
                    contact_name, interaction_type, trace_id
                );

                // Log interaction to database
                self.log_interaction(
                    tenant_id,
                    contact_name,
                    interaction_type,
                    trace_id,
                    cpid,
                    payload.get("context"),
                )
                .await?;

                // Log to telemetry (sampled at 5% per Telemetry Ruleset #9)
                // Citation: Sampling pattern from crates/mplora-profiler/src/lib.rs
                if rand::random::<f32>() < 0.05 || signal.priority == SignalPriority::High {
                    let timestamp = std::time::SystemTime::now()
                        .duration_since(std::time::UNIX_EPOCH)
                        .expect("System time before UNIX epoch")
                        .as_secs();

                    self.telemetry.log(
                        "contact.interaction",
                        ContactInteractionEvent {
                            contact_name: contact_name.to_string(),
                            interaction_type: interaction_type.to_string(),
                            trace_id: trace_id.to_string(),
                            cpid: cpid.to_string(),
                            timestamp,
                        },
                    )?;
                }
            }

            _ => {
                warn!(
                    "ContactDiscoveryHandler received unexpected signal type: {:?}",
                    signal.signal_type
                );
            }
        }

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::signal::{SignalBuilder, SignalPriority};
    use adapteros_platform::common::PlatformUtils;

    fn test_telemetry() -> TelemetryWriter {
        let temp_dir = PlatformUtils::temp_dir().join("mplora_test_telemetry");
        std::fs::create_dir_all(&temp_dir).expect("Test temp directory creation should succeed");
        TelemetryWriter::new(temp_dir, 1000, 1_000_000)
            .expect("Test telemetry writer creation should succeed")
    }

    #[tokio::test]
    async fn test_extract_contact_adapter() {
        let telemetry = test_telemetry();
        let handler = ContactDiscoveryHandler::new(telemetry);

        let signal = SignalBuilder::new(SignalType::ContactDiscovered)
            .priority(SignalPriority::Normal)
            .with_field("name", json!("adapter_0"))
            .with_field("category", json!("adapter"))
            .with_field("tenant_id", json!("test-tenant"))
            .with_field("metadata", json!({"adapter_idx": 0}))
            .trace_id("trace-123")
            .build();

        let contact = handler
            .extract_contact(&signal)
            .expect("Test contact extraction should succeed")
            .expect("Test contact should be Some");
        assert_eq!(contact.name, "adapter_0");
        assert_eq!(contact.category, ContactCategory::Adapter);
        assert_eq!(contact.tenant_id, "test-tenant");
        assert_eq!(contact.discovered_by, Some("trace-123".to_string()));
    }

    #[tokio::test]
    async fn test_extract_contact_repository() {
        let telemetry = test_telemetry();
        let handler = ContactDiscoveryHandler::new(telemetry);

        let signal = SignalBuilder::new(SignalType::ContactDiscovered)
            .priority(SignalPriority::Normal)
            .with_field("name", json!("acme/payments"))
            .with_field("category", json!("repository"))
            .with_field("tenant_id", json!("test-tenant"))
            .with_field("metadata", json!({"repo_path": "/repos/acme/payments"}))
            .build();

        let contact = handler
            .extract_contact(&signal)
            .expect("Test contact extraction should succeed")
            .expect("Test contact should be Some");
        assert_eq!(contact.name, "acme/payments");
        assert_eq!(contact.category, ContactCategory::Repository);
    }

    #[tokio::test]
    async fn test_extract_contact_missing_fields() {
        let telemetry = test_telemetry();
        let handler = ContactDiscoveryHandler::new(telemetry);

        let signal = SignalBuilder::new(SignalType::ContactDiscovered)
            .priority(SignalPriority::Normal)
            .with_field("name", json!("test"))
            // Missing category and tenant_id
            .build();

        let contact = handler
            .extract_contact(&signal)
            .expect("Test contact extraction should succeed");
        assert!(contact.is_none());
    }

    #[tokio::test]
    async fn test_contact_category_conversion() {
        assert_eq!(
            ContactCategory::parse("adapter").expect("Test category parsing should succeed"),
            ContactCategory::Adapter
        );
        assert_eq!(
            ContactCategory::parse("REPOSITORY").expect("Test category parsing should succeed"),
            ContactCategory::Repository
        );
        assert_eq!(
            ContactCategory::parse("User").expect("Test category parsing should succeed"),
            ContactCategory::User
        );
        assert!(ContactCategory::parse("invalid").is_err());
    }
}
