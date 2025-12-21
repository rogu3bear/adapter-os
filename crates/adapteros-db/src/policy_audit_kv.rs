//! KV storage for policy audit decisions with Merkle-style chaining.
//!
//! Keys (per-tenant namespace):
//! - `tenant/{tenant_id}/policy_audit/{id}` -> PolicyAuditDecision (JSON)
//! - `tenant/{tenant_id}/policy_audit/seq/{seq:020}:{id}` -> entry_id (ordering)

use adapteros_core::{AosError, Result};
use adapteros_storage::KvBackend;
use chrono::Utc;
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use uuid::Uuid;

use crate::policy_audit::{ChainVerificationResult, PolicyAuditDecision, PolicyDecisionFilters};

pub struct PolicyAuditKvRepository {
    backend: Arc<dyn KvBackend>,
}

impl PolicyAuditKvRepository {
    pub fn new(backend: Arc<dyn KvBackend>) -> Self {
        Self { backend }
    }

    pub async fn latest_entry(&self, tenant_id: &str) -> Result<Option<PolicyAuditDecision>> {
        if let Some((latest_id, _seq)) = self.latest_for_tenant(tenant_id).await? {
            return self.get_entry(tenant_id, &latest_id).await;
        }
        Ok(None)
    }

    fn entry_key(tenant_id: &str, id: &str) -> String {
        format!("tenant/{}/policy_audit/{}", tenant_id, id)
    }

    fn seq_key(tenant_id: &str, seq: i64, id: &str) -> String {
        format!("tenant/{}/policy_audit/seq/{:020}:{}", tenant_id, seq, id)
    }

    fn seq_prefix(tenant_id: &str) -> String {
        format!("tenant/{}/policy_audit/seq/", tenant_id)
    }

    fn now_ts() -> String {
        Utc::now().to_rfc3339()
    }

    async fn latest_for_tenant(&self, tenant_id: &str) -> Result<Option<(String, i64)>> {
        let mut max_seq: Option<(String, i64)> = None;
        for key in self
            .backend
            .scan_prefix(&Self::seq_prefix(tenant_id))
            .await
            .map_err(|e| AosError::Database(format!("Failed to scan policy audit seq: {}", e)))?
        {
            // key format: .../seq/{seq}:{id}
            if let Some(pos) = key.rsplit_once('/') {
                let seq_part = pos.1;
                if let Some((seq_str, entry_id)) = seq_part.split_once(':') {
                    if let Ok(seq_num) = seq_str.parse::<i64>() {
                        if max_seq.as_ref().map(|(_, s)| seq_num > *s).unwrap_or(true) {
                            max_seq = Some((entry_id.to_string(), seq_num));
                        }
                    }
                }
            }
        }
        Ok(max_seq)
    }

    pub async fn log_policy_decision(
        &self,
        tenant_id: &str,
        policy_pack_id: &str,
        hook: &str,
        decision: &str,
        reason: Option<&str>,
        request_id: Option<&str>,
        user_id: Option<&str>,
        resource_type: Option<&str>,
        resource_id: Option<&str>,
        metadata_json: Option<&str>,
    ) -> Result<String> {
        let id = Uuid::now_v7().to_string();
        let timestamp = Self::now_ts();

        let latest = self.latest_for_tenant(tenant_id).await?;
        let (previous_hash, chain_sequence) = match latest {
            Some((latest_id, seq)) => {
                // load previous entry hash
                let prev_entry = self.get_entry(tenant_id, &latest_id).await?;
                let prev_hash = prev_entry
                    .as_ref()
                    .map(|e| e.entry_hash.clone())
                    .unwrap_or_default();
                (Some(prev_hash), seq + 1)
            }
            None => (None, 1),
        };

        let entry = PolicyAuditDecision {
            id: id.clone(),
            tenant_id: tenant_id.to_string(),
            policy_pack_id: policy_pack_id.to_string(),
            hook: hook.to_string(),
            decision: decision.to_string(),
            reason: reason.map(|s| s.to_string()),
            request_id: request_id.map(|s| s.to_string()),
            user_id: user_id.map(|s| s.to_string()),
            resource_type: resource_type.map(|s| s.to_string()),
            resource_id: resource_id.map(|s| s.to_string()),
            metadata_json: metadata_json.map(|s| s.to_string()),
            timestamp,
            entry_hash: String::new(), // overwritten in put_entry
            previous_hash,
            chain_sequence,
        };

        self.put_entry(&entry).await.map(|()| id)
    }

    pub async fn put_entry(&self, entry: &PolicyAuditDecision) -> Result<()> {
        let entry_data = format!(
            "{}|{}|{}|{}|{}|{}|{}|{}|{}|{}|{}|{}|{}",
            entry.id,
            entry.timestamp,
            entry.tenant_id,
            entry.policy_pack_id,
            entry.hook,
            entry.decision,
            entry.reason.as_deref().unwrap_or(""),
            entry.request_id.as_deref().unwrap_or(""),
            entry.user_id.as_deref().unwrap_or(""),
            entry.resource_type.as_deref().unwrap_or(""),
            entry.resource_id.as_deref().unwrap_or(""),
            entry.metadata_json.as_deref().unwrap_or(""),
            entry.previous_hash.as_deref().unwrap_or(""),
        );
        let mut entry = entry.clone();
        entry.entry_hash = adapteros_core::B3Hash::hash(entry_data.as_bytes()).to_string();

        let payload = serde_json::to_vec(&entry).map_err(AosError::Serialization)?;
        self.backend
            .set(&Self::entry_key(&entry.tenant_id, &entry.id), payload)
            .await
            .map_err(|e| {
                AosError::Database(format!("Failed to store policy audit entry: {}", e))
            })?;
        self.backend
            .set(
                &Self::seq_key(&entry.tenant_id, entry.chain_sequence, &entry.id),
                entry.id.as_bytes().to_vec(),
            )
            .await
            .map_err(|e| AosError::Database(format!("Failed to store policy audit seq: {}", e)))
    }

    pub async fn get_entry(
        &self,
        tenant_id: &str,
        id: &str,
    ) -> Result<Option<PolicyAuditDecision>> {
        let Some(bytes) = self
            .backend
            .get(&Self::entry_key(tenant_id, id))
            .await
            .map_err(|e| AosError::Database(format!("Failed to read policy audit entry: {}", e)))?
        else {
            return Ok(None);
        };
        serde_json::from_slice(&bytes)
            .map_err(AosError::Serialization)
            .map(Some)
    }

    fn parse_seq_key(key: &str) -> Option<(i64, String)> {
        let (_, last) = key.rsplit_once('/')?;
        let (seq_str, id) = last.split_once(':')?;
        let seq = seq_str.parse::<i64>().ok()?;
        Some((seq, id.to_string()))
    }

    pub async fn verify_policy_audit_chain(
        &self,
        tenant_id: Option<&str>,
    ) -> Result<ChainVerificationResult> {
        let mut decisions: Vec<PolicyAuditDecision> = Vec::new();
        if let Some(tid) = tenant_id {
            for key in self
                .backend
                .scan_prefix(&Self::seq_prefix(tid))
                .await
                .map_err(|e| {
                    AosError::Database(format!("Failed to scan policy audit seq: {}", e))
                })?
            {
                if let Some((_, id)) = Self::parse_seq_key(&key) {
                    if let Some(entry) = self.get_entry(tid, &id).await? {
                        decisions.push(entry);
                    }
                }
            }
            decisions.sort_by(|a, b| a.chain_sequence.cmp(&b.chain_sequence));
            return Self::verify_chain_for_tenant(tid, decisions);
        }

        // all tenants: scan tenant prefix
        let mut tenants: std::collections::HashMap<String, Vec<PolicyAuditDecision>> =
            std::collections::HashMap::new();
        for key in
            self.backend.scan_prefix("tenant/").await.map_err(|e| {
                AosError::Database(format!("Failed to scan policy audit keys: {}", e))
            })?
        {
            if key.contains("/policy_audit/seq/") {
                if let Some((tenant_part, _)) = key.split_once("/policy_audit/seq/") {
                    let tenant_id = tenant_part.trim_start_matches("tenant/").to_string();
                    if let Some((_, id)) = Self::parse_seq_key(&key) {
                        if let Some(entry) = self.get_entry(&tenant_id, &id).await? {
                            tenants.entry(tenant_id).or_default().push(entry);
                        }
                    }
                }
            }
        }

        let mut total_checked = 0;
        for (tenant, mut list) in tenants {
            list.sort_by(|a, b| a.chain_sequence.cmp(&b.chain_sequence));
            let result = Self::verify_chain_for_tenant(&tenant, list)?;
            if !result.is_valid {
                return Ok(result);
            }
            total_checked += result.entries_checked;
        }

        Ok(ChainVerificationResult {
            is_valid: true,
            entries_checked: total_checked,
            first_invalid_sequence: None,
            error_message: None,
        })
    }

    fn verify_chain_for_tenant(
        _tenant: &str,
        chain: Vec<PolicyAuditDecision>,
    ) -> Result<ChainVerificationResult> {
        if chain.is_empty() {
            return Ok(ChainVerificationResult {
                is_valid: true,
                entries_checked: 0,
                first_invalid_sequence: None,
                error_message: None,
            });
        }

        let mut prev_hash: Option<String> = None;
        let mut prev_seq = 0i64;
        let mut checked = 0usize;

        for decision in chain {
            checked += 1;
            if decision.chain_sequence != prev_seq + 1 {
                return Ok(ChainVerificationResult {
                    is_valid: false,
                    entries_checked: checked,
                    first_invalid_sequence: Some(decision.chain_sequence),
                    error_message: Some(format!(
                        "Sequence gap: expected {}, got {}",
                        prev_seq + 1,
                        decision.chain_sequence
                    )),
                });
            }

            if let Some(expected_prev) = prev_hash.as_ref() {
                if decision.previous_hash.as_deref() != Some(expected_prev) {
                    return Ok(ChainVerificationResult {
                        is_valid: false,
                        entries_checked: checked,
                        first_invalid_sequence: Some(decision.chain_sequence),
                        error_message: Some("Previous hash mismatch".to_string()),
                    });
                }
            } else if decision.previous_hash.is_some() {
                return Ok(ChainVerificationResult {
                    is_valid: false,
                    entries_checked: checked,
                    first_invalid_sequence: Some(decision.chain_sequence),
                    error_message: Some("First entry has non-null previous_hash".to_string()),
                });
            }

            let entry_data = format!(
                "{}|{}|{}|{}|{}|{}|{}|{}|{}|{}|{}|{}|{}",
                decision.id,
                decision.timestamp,
                decision.tenant_id,
                decision.policy_pack_id,
                decision.hook,
                decision.decision,
                decision.reason.as_deref().unwrap_or(""),
                decision.request_id.as_deref().unwrap_or(""),
                decision.user_id.as_deref().unwrap_or(""),
                decision.resource_type.as_deref().unwrap_or(""),
                decision.resource_id.as_deref().unwrap_or(""),
                decision.metadata_json.as_deref().unwrap_or(""),
                decision.previous_hash.as_deref().unwrap_or(""),
            );
            let computed_hash = adapteros_core::B3Hash::hash(entry_data.as_bytes()).to_string();
            if computed_hash != decision.entry_hash {
                return Ok(ChainVerificationResult {
                    is_valid: false,
                    entries_checked: checked,
                    first_invalid_sequence: Some(decision.chain_sequence),
                    error_message: Some("Entry hash mismatch - possible tampering".to_string()),
                });
            }

            prev_hash = Some(decision.entry_hash.clone());
            prev_seq = decision.chain_sequence;
        }

        Ok(ChainVerificationResult {
            is_valid: true,
            entries_checked: checked,
            first_invalid_sequence: None,
            error_message: None,
        })
    }

    pub async fn query_policy_decisions(
        &self,
        filters: PolicyDecisionFilters,
    ) -> Result<Vec<PolicyAuditDecision>> {
        // For KV we scan per-tenant; filters.tenant_id is required for isolation.
        let tenant_id = filters.tenant_id.clone().ok_or_else(|| {
            AosError::Validation("tenant_id is required for KV policy audit query".to_string())
        })?;

        let mut decisions = Vec::new();
        for key in self
            .backend
            .scan_prefix(&Self::seq_prefix(&tenant_id))
            .await
            .map_err(|e| AosError::Database(format!("Failed to scan policy audit seq: {}", e)))?
        {
            if let Some((_, id)) = Self::parse_seq_key(&key) {
                if let Some(entry) = self.get_entry(&tenant_id, &id).await? {
                    decisions.push(entry);
                }
            }
        }

        // Apply filters in-memory
        decisions.retain(|d| {
            (filters
                .policy_pack_id
                .as_ref()
                .is_none_or(|v| &d.policy_pack_id == v))
                && (filters.hook.as_ref().is_none_or(|v| &d.hook == v))
                && (filters.decision.as_ref().is_none_or(|v| &d.decision == v))
                && (filters
                    .from_time
                    .as_ref()
                    .is_none_or(|v| &d.timestamp >= v))
                && (filters.to_time.as_ref().is_none_or(|v| &d.timestamp <= v))
        });

        decisions.sort_by(|a, b| b.timestamp.cmp(&a.timestamp));

        let offset = filters.offset.unwrap_or(0).max(0) as usize;
        let limit = filters.limit.unwrap_or(100).min(1000) as usize;
        let end = (offset + limit).min(decisions.len());
        let sliced = decisions.get(offset..end).unwrap_or_default().to_vec();
        Ok(sliced)
    }
}
