use crate::B3Hash;
use chrono::{DateTime, NaiveDateTime, Utc};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::BTreeMap;

pub type EventId = String;

/// Snapshot hash result from hydration
#[derive(Debug, Clone, Eq, PartialEq, Serialize, Deserialize)]
pub struct SnapshotHash {
    pub tenant_id: String,
    pub state_hash: B3Hash,
}

#[derive(Debug, Clone, Eq, PartialEq, Serialize, Deserialize)]
pub struct TenantStateSnapshot {
    pub tenant_id: String,
    pub adapters: Vec<AdapterInfo>,
    pub stacks: Vec<StackInfo>,
    pub router_policies: Vec<PolicyInfo>,
    pub plugin_configs: std::collections::BTreeMap<String, Value>,
    pub feature_flags: std::collections::BTreeMap<String, bool>,
    pub configs: std::collections::BTreeMap<String, Value>,
    pub snapshot_timestamp: DateTime<Utc>,
}

#[derive(Debug, Clone, Eq, PartialEq, Serialize, Deserialize)]
pub struct AdapterInfo {
    pub id: String,
    pub name: String,
    pub rank: u32,
    pub version: String,
}

#[derive(Debug, Clone, Eq, PartialEq, Serialize, Deserialize)]
pub struct StackInfo {
    pub name: String,
    pub adapter_ids: Vec<String>,
}

#[derive(Debug, Clone, Eq, PartialEq, Serialize, Deserialize)]
pub struct PolicyInfo {
    pub name: String,
    pub rules: Vec<String>,
}

impl TenantStateSnapshot {
    /// Compute deterministic hash over canonical snapshot representation
    ///
    /// Invariants:
    /// - Adapters sorted by ID
    /// - Stacks sorted by name
    /// - Router policies sorted by name
    /// - BTreeMap configs maintain sorted order
    /// - JSON serialization is deterministic (no floating precision issues)
    ///
    /// Same DB contents MUST yield identical hash across runs/machines/architectures.
    pub fn compute_hash(&self) -> B3Hash {
        // Clone and enforce canonical ordering
        let mut canonical = self.clone();
        canonical.adapters.sort_by(|a, b| a.id.cmp(&b.id));
        canonical.stacks.sort_by(|a, b| a.name.cmp(&b.name));
        canonical.router_policies.sort_by(|a, b| a.name.cmp(&b.name));

        // Sort adapter_ids within each stack for determinism
        for stack in &mut canonical.stacks {
            stack.adapter_ids.sort();
        }

        // Sort rules within each policy for determinism
        for policy in &mut canonical.router_policies {
            policy.rules.sort();
        }

        // Serialize to canonical JSON (BTreeMap ensures sorted keys)
        let json = serde_json::to_string(&canonical).expect("Serialization failed");
        B3Hash::hash(json.as_bytes())
    }

    pub fn from_bundle_events(events: &[Value]) -> Self {
        let mut sorted_events: Vec<_> = events.iter().enumerate().collect();
        sorted_events.sort_by(|(_, e1), (_, e2)| {
            let ts1 = parse_timestamp(e1.get("timestamp").unwrap_or(&Value::Null));
            let ts2 = parse_timestamp(e2.get("timestamp").unwrap_or(&Value::Null));
            ts1.cmp(&ts2).then_with(|| {
                e1.get("event_type")
                    .unwrap_or(&Value::Null)
                    .as_str()
                    .unwrap_or("")
                    .cmp(
                        e2.get("event_type")
                            .unwrap_or(&Value::Null)
                            .as_str()
                            .unwrap_or(""),
                    )
            })
        });

        let tenant_id = if let Some(first) = sorted_events.first() {
            first
                .1
                .get("identity")
                .and_then(|i| i.get("tenant_id"))
                .and_then(|t| t.as_str())
                .unwrap_or("unknown")
                .to_string()
        } else {
            "unknown".to_string()
        };

        let mut adapters = vec![];
        let mut stacks = vec![];
        let mut router_policies = vec![];
        let mut plugin_configs = BTreeMap::new();
        let mut feature_flags = BTreeMap::new();
        let mut configs = BTreeMap::new();
        let mut max_ts = Utc::now().naive_utc();

        for (_, event) in sorted_events {
            if let Some(event_type) = event.get("event_type").and_then(|t| t.as_str()) {
                match event_type {
                    "adapter.registered" => {
                        if let Some(meta) = event.get("metadata") {
                            let id = meta
                                .get("id")
                                .and_then(|i| i.as_str())
                                .unwrap_or("unknown")
                                .to_string();
                            let name = meta
                                .get("name")
                                .and_then(|n| n.as_str())
                                .unwrap_or("unknown")
                                .to_string();
                            let rank =
                                meta.get("rank").and_then(|r| r.as_u64()).unwrap_or(0) as u32;
                            let version = meta
                                .get("version")
                                .and_then(|v| v.as_str())
                                .unwrap_or("0.0")
                                .to_string();
                            adapters.push(AdapterInfo {
                                id: id.clone(),
                                name,
                                rank,
                                version,
                            });
                        }
                    }
                    "stack.created" => {
                        if let Some(meta) = event.get("metadata") {
                            let name = meta
                                .get("name")
                                .and_then(|n| n.as_str())
                                .unwrap_or("unknown")
                                .to_string();
                            let ids: Vec<String> = meta
                                .get("adapter_ids")
                                .and_then(|a| a.as_array())
                                .map(|arr| {
                                    arr.iter()
                                        .filter_map(|v| v.as_str().map(|s| s.to_string()))
                                        .collect()
                                })
                                .unwrap_or_default();
                            stacks.push(StackInfo {
                                name,
                                adapter_ids: ids,
                            });
                        }
                    }
                    "policy.updated" => {
                        if let Some(meta) = event.get("metadata") {
                            let name = meta
                                .get("name")
                                .and_then(|n| n.as_str())
                                .unwrap_or("unknown")
                                .to_string();
                            let rules: Vec<String> = meta
                                .get("rules")
                                .and_then(|r| r.as_array())
                                .map(|arr| {
                                    arr.iter()
                                        .filter_map(|v| v.as_str().map(|s| s.to_string()))
                                        .collect()
                                })
                                .unwrap_or_default();
                            router_policies.push(PolicyInfo { name, rules });
                        }
                    }
                    "config.updated" => {
                        if let Some(meta) = event.get("metadata") {
                            if let (Some(key), Some(value)) =
                                (meta.get("key").and_then(|k| k.as_str()), meta.get("value"))
                            {
                                configs.insert(key.to_string(), value.clone());
                            }
                        }
                    }
                    "plugin.config.updated" => {
                        if let Some(meta) = event.get("metadata") {
                            if let (Some(plugin), Some(config)) = (
                                meta.get("plugin").and_then(|p| p.as_str()),
                                meta.get("config"),
                            ) {
                                plugin_configs.insert(plugin.to_string(), config.clone());
                            }
                        }
                    }
                    "feature.flag.toggled" => {
                        if let Some(meta) = event.get("metadata") {
                            if let (Some(flag), Some(enabled)) = (
                                meta.get("flag").and_then(|f| f.as_str()),
                                meta.get("enabled").and_then(|e| e.as_bool()),
                            ) {
                                feature_flags.insert(flag.to_string(), enabled);
                            }
                        }
                    }
                    _ => {} // Ignore other events
                }

                // Update max timestamp
                if let Some(ts_str) = event.get("timestamp").and_then(|t| t.as_str()) {
                    if let Ok(ts) = DateTime::parse_from_rfc3339(ts_str) {
                        let naive = ts.naive_utc();
                        if naive > max_ts {
                            max_ts = naive;
                        }
                    }
                }
            }
        }

        // Canonical sorts
        adapters.sort_by(|a, b| a.id.cmp(&b.id));
        stacks.sort_by(|s1, s2| s1.name.cmp(&s2.name));
        router_policies.sort_by(|p1, p2| p1.name.cmp(&p2.name));

        let snapshot_timestamp = DateTime::from_naive_utc_and_offset(max_ts, Utc);

        Self {
            tenant_id,
            adapters,
            stacks,
            router_policies,
            plugin_configs,
            feature_flags,
            configs,
            snapshot_timestamp,
        }
    }
}

fn parse_timestamp(val: &Value) -> NaiveDateTime {
    if let Some(ts) = val.as_str() {
        if let Ok(dt) = DateTime::parse_from_rfc3339(ts) {
            return dt.naive_utc();
        }
    }
    Utc::now().naive_utc()
}
