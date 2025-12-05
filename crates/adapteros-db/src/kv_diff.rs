//! Dual-write diff utilities for KV vs SQL

use crate::{
    kv_backend::KvBackend, plans_kv::PlanKvRepository, stacks_kv::StackKvOps,
    stacks_kv::StackKvRepository, tenant_policy_bindings_kv::PolicyBindingKvRepository,
    tenants_kv::TenantKvOps, tenants_kv::TenantKvRepository, users_kv::UserKvOps,
    users_kv::UserKvRepository, Db,
};
use crate::adapters_kv::AdapterKvOps;
use adapteros_core::AosError;
use std::collections::HashMap;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DiffIssue {
    pub domain: String,
    pub id: String,
    pub field: String,
    pub sql_value: String,
    pub kv_value: String,
}

impl Db {
    pub async fn diff_users(&self) -> adapteros_core::Result<Vec<DiffIssue>> {
        let mut issues = Vec::new();
        let Some(pool) = self.pool_opt() else {
            return Ok(issues);
        };
        let sql_users = sqlx::query!(
            r#"SELECT id, email, display_name, pw_hash, role, disabled, created_at, tenant_id FROM users"#
        )
        .fetch_all(pool)
        .await?;

        let Some(kv) = self.kv_backend() else {
            return Ok(issues);
        };
        let repo = UserKvRepository::new((**kv).clone());

        for row in sql_users {
            let row_id = row.id.clone().unwrap_or_default();
            let kv_user = repo.get_user_kv(&row_id).await?;
            if let Some(kv_user) = kv_user {
                if kv_user.email != row.email {
                    issues.push(DiffIssue {
                        domain: "users".into(),
                        id: row_id.clone(),
                        field: "email".into(),
                        sql_value: row.email.clone(),
                        kv_value: kv_user.email.clone(),
                    });
                }
                if kv_user.role.to_string() != row.role {
                    issues.push(DiffIssue {
                        domain: "users".into(),
                        id: row_id.clone(),
                        field: "role".into(),
                        sql_value: row.role.clone(),
                        kv_value: kv_user.role.to_string(),
                    });
                }
            } else {
                issues.push(DiffIssue {
                    domain: "users".into(),
                    id: row_id.clone(),
                    field: "_existence".into(),
                    sql_value: "present".into(),
                    kv_value: "missing".into(),
                });
            }
        }

        Ok(issues)
    }

    pub async fn diff_tenants(&self) -> adapteros_core::Result<Vec<DiffIssue>> {
        let mut issues = Vec::new();
        let Some(pool) = self.pool_opt() else {
            return Ok(issues);
        };
        let sql_tenants = sqlx::query!(
            r#"SELECT id, name, itar_flag, status, default_stack_id FROM tenants"#
        )
        .fetch_all(pool)
        .await?;

        let Some(kv) = self.kv_backend() else {
            return Ok(issues);
        };
        let repo = TenantKvRepository::new(kv.backend().clone());

        for row in sql_tenants {
            let row_id = row.id.clone().unwrap_or_default();
            let kv_tenant = repo.get_tenant_kv(&row_id).await?;
            if let Some(kv_tenant) = kv_tenant {
                if kv_tenant.name != row.name {
                    issues.push(DiffIssue {
                        domain: "tenants".into(),
                        id: row_id.clone(),
                        field: "name".into(),
                        sql_value: row.name.clone(),
                        kv_value: kv_tenant.name.clone(),
                    });
                }
                if kv_tenant
                    .default_stack_id
                    .as_deref()
                    != row.default_stack_id.as_deref()
                {
                    issues.push(DiffIssue {
                        domain: "tenants".into(),
                        id: row_id.clone(),
                        field: "default_stack_id".into(),
                        sql_value: row
                            .default_stack_id
                            .clone()
                            .unwrap_or_else(|| "None".into()),
                        kv_value: kv_tenant
                            .default_stack_id
                            .clone()
                            .unwrap_or_else(|| "None".into()),
                    });
                }
            } else {
                issues.push(DiffIssue {
                    domain: "tenants".into(),
                    id: row_id.clone(),
                    field: "_existence".into(),
                    sql_value: "present".into(),
                    kv_value: "missing".into(),
                });
            }
        }

        Ok(issues)
    }

    pub async fn diff_plans(&self) -> adapteros_core::Result<Vec<DiffIssue>> {
        let mut issues = Vec::new();
        let Some(pool) = self.pool_opt() else {
            return Ok(issues);
        };
        let sql_plans = sqlx::query!(
            r#"SELECT id, tenant_id, plan_id_b3, manifest_hash_b3, kernel_hashes_json FROM plans"#
        )
        .fetch_all(pool)
        .await?;

        let Some(kv) = self.kv_backend() else {
            return Ok(issues);
        };
        let repo = PlanKvRepository::new(kv.backend().clone());
        let kv_all = repo.list_all().await?;

        for row in sql_plans {
            let row_id = row.id.clone().unwrap_or_default();
            let kv_plan = kv_all.iter().find(|p| p.id == row_id);
            if let Some(kv_plan) = kv_plan {
                if kv_plan.plan_id_b3 != row.plan_id_b3 {
                    issues.push(DiffIssue {
                        domain: "plans".into(),
                        id: row_id.clone(),
                        field: "plan_id_b3".into(),
                        sql_value: row.plan_id_b3.clone(),
                        kv_value: kv_plan.plan_id_b3.clone(),
                    });
                }
                if kv_plan.manifest_hash_b3 != row.manifest_hash_b3 {
                    issues.push(DiffIssue {
                        domain: "plans".into(),
                        id: row_id.clone(),
                        field: "manifest_hash_b3".into(),
                        sql_value: row.manifest_hash_b3.clone(),
                        kv_value: kv_plan.manifest_hash_b3.clone(),
                    });
                }
            } else {
                issues.push(DiffIssue {
                    domain: "plans".into(),
                    id: row_id.clone(),
                    field: "_existence".into(),
                    sql_value: "present".into(),
                    kv_value: "missing".into(),
                });
            }
        }

        Ok(issues)
    }

    pub async fn diff_adapters(&self) -> adapteros_core::Result<Vec<DiffIssue>> {
        let mut issues = Vec::new();
        let Some(pool) = self.pool_opt() else {
            return Ok(issues);
        };

        let sql_adapters = sqlx::query!(
            r#"SELECT id, tenant_id, name, hash_b3, tier, created_at FROM adapters WHERE active = 1"#
        )
        .fetch_all(pool)
        .await?;

        let mut cache: HashMap<String, HashMap<String, crate::adapters::Adapter>> = HashMap::new();

        for row in sql_adapters {
            let kv_map = if let Some(existing) = cache.get(&row.tenant_id) {
                existing.clone()
            } else {
                let Some(kv_repo) = self.get_adapter_kv_repo(&row.tenant_id) else {
                    continue;
                };
                let map: HashMap<String, crate::adapters::Adapter> = kv_repo
                    .list_adapters_for_tenant_kv(&row.tenant_id)
                    .await?
                    .into_iter()
                    .map(|a| (a.id.clone(), a))
                    .collect();
                cache.insert(row.tenant_id.clone(), map.clone());
                map
            };
            let row_id = row.id.clone().unwrap_or_default();
            match kv_map.get(&row_id) {
                Some(kv_adapter) => {
                    if kv_adapter.name != row.name {
                        issues.push(DiffIssue {
                            domain: "adapters".into(),
                            id: row_id.clone(),
                            field: "name".into(),
                            sql_value: row.name.clone(),
                            kv_value: kv_adapter.name.clone(),
                        });
                    }
                    if kv_adapter.hash_b3 != row.hash_b3 {
                        issues.push(DiffIssue {
                            domain: "adapters".into(),
                            id: row_id.clone(),
                            field: "hash_b3".into(),
                            sql_value: row.hash_b3.clone(),
                            kv_value: kv_adapter.hash_b3.clone(),
                        });
                    }
                    if kv_adapter.tier != row.tier {
                        issues.push(DiffIssue {
                            domain: "adapters".into(),
                            id: row_id.clone(),
                            field: "tier".into(),
                            sql_value: row.tier.clone(),
                            kv_value: kv_adapter.tier.clone(),
                        });
                    }
                }
                None => issues.push(DiffIssue {
                    domain: "adapters".into(),
                    id: row_id.clone(),
                    field: "_existence".into(),
                    sql_value: "present".into(),
                    kv_value: "missing".into(),
                }),
            }
        }

        Ok(issues)
    }

    pub async fn diff_stacks(&self) -> adapteros_core::Result<Vec<DiffIssue>> {
        let mut issues = Vec::new();
        let Some(pool) = self.pool_opt() else {
            return Ok(issues);
        };

        let sql_stacks = sqlx::query!(
            r#"SELECT id, tenant_id, name, description, lifecycle_state FROM adapter_stacks"#
        )
        .fetch_all(pool)
        .await?;

        let Some(kv) = self.kv_backend() else {
            return Ok(issues);
        };
        let kv_repo = StackKvRepository::new(kv.backend().clone());

        for row in sql_stacks {
            let kv_stack = kv_repo
                .get_stack(&row.tenant_id, row.id.as_deref().unwrap_or_default())
                .await?;
            let row_id = row.id.clone().unwrap_or_default();
            match kv_stack {
                Some(stack) => {
                    if stack.name != row.name {
                        issues.push(DiffIssue {
                            domain: "adapter_stacks".into(),
                            id: row_id.clone(),
                            field: "name".into(),
                            sql_value: row.name.clone(),
                            kv_value: stack.name.clone(),
                        });
                    }
                    let kv_state = stack.lifecycle_state.to_string();
                    if kv_state != row.lifecycle_state {
                        issues.push(DiffIssue {
                            domain: "adapter_stacks".into(),
                            id: row_id.clone(),
                            field: "lifecycle_state".into(),
                            sql_value: row.lifecycle_state.clone(),
                            kv_value: kv_state,
                        });
                    }
                }
                None => issues.push(DiffIssue {
                    domain: "adapter_stacks".into(),
                    id: row_id.clone(),
                    field: "_existence".into(),
                    sql_value: "present".into(),
                    kv_value: "missing".into(),
                }),
            }
        }

        Ok(issues)
    }

    pub async fn diff_policy_bindings(&self) -> adapteros_core::Result<Vec<DiffIssue>> {
        let mut issues = Vec::new();
        let Some(pool) = self.pool_opt() else {
            return Ok(issues);
        };

        let sql_bindings = sqlx::query!(
            r#"SELECT tenant_id, policy_pack_id, enabled FROM tenant_policy_bindings WHERE scope = 'global'"#
        )
        .fetch_all(pool)
        .await?;

        let Some(kv) = self.kv_backend() else {
            return Ok(issues);
        };
        let repo = PolicyBindingKvRepository::new(kv.backend().clone());

        for row in sql_bindings {
            let kv_binding = repo
                .get_binding(&row.tenant_id, &row.policy_pack_id)
                .await?;
            match kv_binding {
                Some(binding) => {
                    if binding.enabled != (row.enabled != 0) {
                        issues.push(DiffIssue {
                            domain: "tenant_policy_bindings".into(),
                            id: row.policy_pack_id.clone(),
                            field: "enabled".into(),
                            sql_value: format!("{}", row.enabled != 0),
                            kv_value: format!("{}", binding.enabled),
                        });
                    }
                }
                None => issues.push(DiffIssue {
                    domain: "tenant_policy_bindings".into(),
                    id: row.policy_pack_id.clone(),
                    field: "_existence".into(),
                    sql_value: "present".into(),
                    kv_value: "missing".into(),
                }),
            }
        }

        Ok(issues)
    }

    pub async fn diff_all_supported(&self) -> adapteros_core::Result<Vec<DiffIssue>> {
        if !self.has_kv_backend() {
            return Err(AosError::Config(
                "KV backend not attached; cannot diff".to_string(),
            ));
        }

        let mut issues = Vec::new();
        issues.extend(self.diff_users().await?);
        issues.extend(self.diff_tenants().await?);
        issues.extend(self.diff_plans().await?);
        issues.extend(self.diff_adapters().await?);
        issues.extend(self.diff_stacks().await?);
        issues.extend(self.diff_policy_bindings().await?);
        Ok(issues)
    }
}

