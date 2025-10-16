//! TTL management for ephemeral adapters.
//!
//! The lifecycle crate owns eviction policy enforcement.  The TTL manager
//! keeps track of adapters with expiration times and provides deterministic
//! eviction ordering along with an audit log.

use chrono::{DateTime, Duration, Utc};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, VecDeque};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TtlRecord {
    pub adapter_id: String,
    pub tenant_id: String,
    pub expires_at: DateTime<Utc>,
    pub ttl_hours: u64,
    pub last_extension: Option<DateTime<Utc>>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EvictionAuditEntry {
    pub adapter_id: String,
    pub tenant_id: String,
    pub evicted_at: DateTime<Utc>,
    pub reason: String,
}

#[derive(Default)]
pub struct TtlManager {
    records: BTreeMap<String, TtlRecord>,
    eviction_audit: VecDeque<EvictionAuditEntry>,
    audit_capacity: usize,
}

impl TtlManager {
    pub fn new(audit_capacity: usize) -> Self {
        Self {
            records: BTreeMap::new(),
            eviction_audit: VecDeque::new(),
            audit_capacity,
        }
    }

    pub fn track(&mut self, record: TtlRecord) {
        self.records.insert(record.adapter_id.clone(), record);
    }

    pub fn extend_ttl(&mut self, adapter_id: &str, additional_hours: u64) {
        if let Some(record) = self.records.get_mut(adapter_id) {
            let max_expiry = Utc::now() + Duration::hours(72);
            let new_expiry = record.expires_at + Duration::hours(additional_hours as i64);
            record.expires_at = std::cmp::min(new_expiry, max_expiry);
            record.last_extension = Some(Utc::now());
        }
    }

    pub fn evict_expired(&mut self, now: DateTime<Utc>) -> Vec<EvictionAuditEntry> {
        let expired: Vec<String> = self
            .records
            .iter()
            .filter(|(_, record)| record.expires_at <= now)
            .map(|(id, _)| id.clone())
            .collect();

        let mut audit_entries = Vec::new();
        for id in expired {
            if let Some(record) = self.records.remove(&id) {
                let entry = EvictionAuditEntry {
                    adapter_id: record.adapter_id.clone(),
                    tenant_id: record.tenant_id.clone(),
                    evicted_at: now,
                    reason: "ttl_expired".into(),
                };
                self.push_audit(entry.clone());
                audit_entries.push(entry);
            }
        }
        audit_entries
    }

    pub fn active_records(&self) -> Vec<&TtlRecord> {
        self.records.values().collect()
    }

    pub fn audit_log(&self) -> Vec<EvictionAuditEntry> {
        self.eviction_audit.iter().cloned().collect()
    }

    fn push_audit(&mut self, entry: EvictionAuditEntry) {
        if self.eviction_audit.len() == self.audit_capacity {
            self.eviction_audit.pop_front();
        }
        self.eviction_audit.push_back(entry);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn eviction_records_are_logged() {
        let mut manager = TtlManager::new(10);
        let now = Utc::now();
        manager.track(TtlRecord {
            adapter_id: "a".into(),
            tenant_id: "tenant".into(),
            expires_at: now + Duration::hours(1),
            ttl_hours: 24,
            last_extension: None,
        });

        let audit = manager.evict_expired(now + Duration::hours(5));
        assert_eq!(audit.len(), 1);
        assert_eq!(audit[0].adapter_id, "a");
        assert_eq!(manager.audit_log().len(), 1);
    }

    #[test]
    fn extension_is_capped() {
        let mut manager = TtlManager::new(5);
        let now = Utc::now();
        manager.track(TtlRecord {
            adapter_id: "a".into(),
            tenant_id: "tenant".into(),
            expires_at: now + Duration::hours(10),
            ttl_hours: 24,
            last_extension: None,
        });
        manager.extend_ttl("a", 200);
        let record = manager.records.get("a").unwrap();
        assert!(record.expires_at <= now + Duration::hours(72));
        assert!(record.last_extension.is_some());
    }
}
