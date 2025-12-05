//! Dual-write diff utilities for KV vs SQL

use crate::{
    kv_backend::KvBackend, plans_kv::PlanKvRepository, tenants_kv::TenantKvOps,
    tenants_kv::TenantKvRepository, users_kv::UserKvOps, users_kv::UserKvRepository, Db,
};
use adapteros_core::AosError;
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
        Ok(issues)
    }
}

