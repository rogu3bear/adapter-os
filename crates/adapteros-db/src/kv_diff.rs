//! Dual-write diff utilities for KV vs SQL

use crate::adapters_kv::AdapterKvOps;
use crate::auth_sessions_kv::AuthSessionKvRepository;
use crate::chat_sessions_kv::ChatSessionKvRepository;
use crate::collections_kv::CollectionKvRepository;
use crate::documents_kv::DocumentKvRepository;
use crate::kv_backend::KvBackend;
use crate::plans_kv::PlanKvRepository;
use crate::policy_audit_kv::PolicyAuditKvRepository;
use crate::runtime_sessions_kv::RuntimeSessionKvRepository;
use crate::stacks_kv::{StackKvOps, StackKvRepository};
use crate::tenant_policy_bindings_kv::PolicyBindingKvRepository;
use crate::tenants_kv::{TenantKvOps, TenantKvRepository};
use crate::training_jobs_kv::TrainingJobKvRepository;
use crate::users_kv::{UserKvOps, UserKvRepository};
use crate::Db;
use adapteros_core::AosError;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DiffIssue {
    pub domain: String,
    pub id: String,
    pub field: String,
    pub sql_value: String,
    pub kv_value: String,
}

impl Db {
    pub async fn diff_documents(&self) -> adapteros_core::Result<Vec<DiffIssue>> {
        let mut issues = Vec::new();
        let Some(pool) = self.pool_opt() else {
            return Ok(issues);
        };
        let sql_docs = sqlx::query!(
            r#"SELECT id, tenant_id, name, content_hash, status, file_path, file_size, mime_type FROM documents"#
        )
        .fetch_all(pool)
        .await?;

        let Some(kv) = self.kv_backend() else {
            return Ok(issues);
        };
        let repo = DocumentKvRepository::new(kv.backend().clone());

        for row in sql_docs {
            let row_id = row.id.clone().unwrap_or_default();
            if let Some(doc) = repo.get_document(&row.tenant_id, &row_id).await? {
                if doc.name != row.name {
                    issues.push(DiffIssue {
                        domain: "documents".into(),
                        id: row_id.clone(),
                        field: "name".into(),
                        sql_value: row.name.clone(),
                        kv_value: doc.name,
                    });
                }
                if doc.status != row.status {
                    issues.push(DiffIssue {
                        domain: "documents".into(),
                        id: row_id.clone(),
                        field: "status".into(),
                        sql_value: row.status.clone(),
                        kv_value: doc.status,
                    });
                }
                if doc.content_hash != row.content_hash {
                    issues.push(DiffIssue {
                        domain: "documents".into(),
                        id: row_id.clone(),
                        field: "content_hash".into(),
                        sql_value: row.content_hash.clone(),
                        kv_value: doc.content_hash.clone(),
                    });
                }
                if doc.file_path != row.file_path {
                    issues.push(DiffIssue {
                        domain: "documents".into(),
                        id: row_id.clone(),
                        field: "file_path".into(),
                        sql_value: row.file_path.clone(),
                        kv_value: doc.file_path.clone(),
                    });
                }
            } else {
                issues.push(DiffIssue {
                    domain: "documents".into(),
                    id: row_id.clone(),
                    field: "_existence".into(),
                    sql_value: "present".into(),
                    kv_value: "missing".into(),
                });
            }
        }

        Ok(issues)
    }

    pub async fn diff_collections(&self) -> adapteros_core::Result<Vec<DiffIssue>> {
        let mut issues = Vec::new();
        let Some(pool) = self.pool_opt() else {
            return Ok(issues);
        };
        let sql_cols = sqlx::query!(
            r#"SELECT id, tenant_id, name, description, created_at, updated_at FROM document_collections"#
        )
        .fetch_all(pool)
        .await?;

        let Some(kv) = self.kv_backend() else {
            return Ok(issues);
        };
        let repo = CollectionKvRepository::new(kv.backend().clone());

        for row in sql_cols {
            let row_id = row.id.clone().unwrap_or_default();
            if let Some(col) = repo.get_collection(&row.tenant_id, &row_id).await? {
                if col.name != row.name {
                    issues.push(DiffIssue {
                        domain: "collections".into(),
                        id: row_id.clone(),
                        field: "name".into(),
                        sql_value: row.name.clone(),
                        kv_value: col.name,
                    });
                }
                if col.description != row.description {
                    issues.push(DiffIssue {
                        domain: "collections".into(),
                        id: row_id.clone(),
                        field: "description".into(),
                        sql_value: row
                            .description
                            .clone()
                            .unwrap_or_else(|| "None".into()),
                        kv_value: col.description.unwrap_or_else(|| "None".into()),
                    });
                }
            } else {
                issues.push(DiffIssue {
                    domain: "collections".into(),
                    id: row_id.clone(),
                    field: "_existence".into(),
                    sql_value: "present".into(),
                    kv_value: "missing".into(),
                });
            }
        }

        Ok(issues)
    }

    pub async fn diff_collection_links(&self) -> adapteros_core::Result<Vec<DiffIssue>> {
        let mut issues = Vec::new();
        let Some(pool) = self.pool_opt() else {
            return Ok(issues);
        };

        #[derive(sqlx::FromRow)]
        struct LinkRow {
            tenant_id: String,
            collection_id: String,
            document_id: String,
        }

        let mut rows = sqlx::query_as::<_, LinkRow>(
            r#"SELECT tenant_id, collection_id, document_id FROM collection_documents"#,
        )
        .fetch_all(pool)
        .await?;

        rows.sort_by(|a, b| {
            a.collection_id
                .cmp(&b.collection_id)
                .then_with(|| a.document_id.cmp(&b.document_id))
        });

        let Some(kv) = self.kv_backend() else {
            return Ok(issues);
        };
        let repo = CollectionKvRepository::new(kv.backend().clone());

        for row in rows {
            let links = repo
                .list_collection_links(&row.tenant_id, &row.collection_id)
                .await
                .unwrap_or_default();
            let exists = links
                .iter()
                .any(|l| l.document_id == row.document_id && l.tenant_id == row.tenant_id);
            if !exists {
                issues.push(DiffIssue {
                    domain: "collection_documents".into(),
                    id: format!("{}:{}", row.collection_id, row.document_id),
                    field: "_existence".into(),
                    sql_value: "present".into(),
                    kv_value: "missing".into(),
                });
            }
        }

        Ok(issues)
    }

    pub async fn diff_policy_audit(&self) -> adapteros_core::Result<Vec<DiffIssue>> {
        let mut issues = Vec::new();
        let Some(pool) = self.pool_opt() else {
            return Ok(issues);
        };
        let sql_entries = sqlx::query!(
            r#"SELECT id, tenant_id, decision, hook, chain_sequence FROM policy_audit_decisions"#
        )
        .fetch_all(pool)
        .await?;

        let Some(kv) = self.kv_backend() else {
            return Ok(issues);
        };
        let repo = PolicyAuditKvRepository::new(kv.backend().clone());

        for row in sql_entries {
            let row_id = row.id.clone().unwrap_or_default();
            if let Some(entry) = repo.get_entry(&row.tenant_id, &row_id).await? {
                if entry.decision != row.decision {
                    issues.push(DiffIssue {
                        domain: "policy_audit".into(),
                        id: row_id.clone(),
                        field: "decision".into(),
                        sql_value: row.decision.clone(),
                        kv_value: entry.decision,
                    });
                }
                let chain_seq = row.chain_sequence;
                if entry.chain_sequence != chain_seq {
                    issues.push(DiffIssue {
                        domain: "policy_audit".into(),
                        id: row_id.clone(),
                        field: "chain_sequence".into(),
                        sql_value: chain_seq.to_string(),
                        kv_value: entry.chain_sequence.to_string(),
                    });
                }
            } else {
                issues.push(DiffIssue {
                    domain: "policy_audit".into(),
                    id: row_id.clone(),
                    field: "_existence".into(),
                    sql_value: "present".into(),
                    kv_value: "missing".into(),
                });
            }
        }

        Ok(issues)
    }

    pub async fn diff_training_jobs(&self) -> adapteros_core::Result<Vec<DiffIssue>> {
        let mut issues = Vec::new();
        let Some(pool) = self.pool_opt() else {
            return Ok(issues);
        };
        let sql_jobs =
            sqlx::query!(r#"SELECT id, tenant_id, repo_id, status FROM repository_training_jobs"#)
                .fetch_all(pool)
                .await?;

        let Some(kv) = self.kv_backend() else {
            return Ok(issues);
        };
        let repo = TrainingJobKvRepository::new(kv.backend().clone());

        for row in sql_jobs {
            let row_id = row.id.clone().unwrap_or_default();
            if let Some(job) = repo.get_job(&row_id).await? {
                let repo_id = row.repo_id.clone();
                if job.repo_id != repo_id {
                    issues.push(DiffIssue {
                        domain: "training_jobs".into(),
                        id: row_id.clone(),
                        field: "repo_id".into(),
                        sql_value: repo_id,
                        kv_value: job.repo_id,
                    });
                }
                if job.status != row.status {
                    issues.push(DiffIssue {
                        domain: "training_jobs".into(),
                        id: row_id.clone(),
                        field: "status".into(),
                        sql_value: row.status.clone(),
                        kv_value: job.status,
                    });
                }
            } else {
                issues.push(DiffIssue {
                    domain: "training_jobs".into(),
                    id: row_id.clone(),
                    field: "_existence".into(),
                    sql_value: "present".into(),
                    kv_value: "missing".into(),
                });
            }
        }

        Ok(issues)
    }

    pub async fn diff_chat_sessions(&self) -> adapteros_core::Result<Vec<DiffIssue>> {
        let mut issues = Vec::new();
        let Some(pool) = self.pool_opt() else {
            return Ok(issues);
        };
        let sql_sessions =
            sqlx::query!(r#"SELECT id, tenant_id, name, last_activity_at FROM chat_sessions"#)
                .fetch_all(pool)
                .await?;

        let Some(kv) = self.kv_backend() else {
            return Ok(issues);
        };
        let repo = ChatSessionKvRepository::new(kv.backend().clone());

        for row in sql_sessions {
            let row_id = row.id.clone().unwrap_or_default();
            if let Some(sess) = repo.get_chat_session(&row_id).await? {
                if sess.name != row.name {
                    issues.push(DiffIssue {
                        domain: "chat_sessions".into(),
                        id: row_id.clone(),
                        field: "name".into(),
                        sql_value: row.name.clone(),
                        kv_value: sess.name,
                    });
                }
                if sess.last_activity_at != row.last_activity_at {
                    issues.push(DiffIssue {
                        domain: "chat_sessions".into(),
                        id: row_id.clone(),
                        field: "last_activity_at".into(),
                        sql_value: row.last_activity_at.clone(),
                        kv_value: sess.last_activity_at,
                    });
                }
            } else {
                issues.push(DiffIssue {
                    domain: "chat_sessions".into(),
                    id: row_id.clone(),
                    field: "_existence".into(),
                    sql_value: "present".into(),
                    kv_value: "missing".into(),
                });
            }
        }

        Ok(issues)
    }

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
        let sql_tenants =
            sqlx::query!(r#"SELECT id, name, itar_flag, status, default_stack_id FROM tenants"#)
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
                if kv_tenant.default_stack_id.as_deref() != row.default_stack_id.as_deref() {
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

    pub async fn diff_auth_sessions(&self) -> adapteros_core::Result<Vec<DiffIssue>> {
        let mut issues = Vec::new();
        let Some(pool) = self.pool_opt() else {
            return Ok(issues);
        };

        #[derive(sqlx::FromRow)]
        struct AuthRow {
            jti: String,
            user_id: String,
            ip_address: Option<String>,
            user_agent: Option<String>,
            last_activity: String,
            expires_at: i64,
        }

        let mut sql_sessions = sqlx::query_as::<_, AuthRow>(
            r#"SELECT jti, user_id, ip_address, user_agent, last_activity, expires_at FROM auth_sessions"#,
        )
        .fetch_all(pool)
        .await?;

        sql_sessions.sort_by(|a, b| {
            b.last_activity
                .cmp(&a.last_activity)
                .then_with(|| a.jti.cmp(&b.jti))
        });

        let Some(kv) = self.kv_backend() else {
            return Ok(issues);
        };
        let repo = AuthSessionKvRepository::new(kv.backend().clone());

        for row in sql_sessions {
            let kv_session = repo.get_session(&row.jti).await?;
            match kv_session {
                Some(sess) => {
                    if sess.user_id != row.user_id {
                        issues.push(DiffIssue {
                            domain: "auth_sessions".into(),
                            id: row.jti.clone(),
                            field: "user_id".into(),
                            sql_value: row.user_id.clone(),
                            kv_value: sess.user_id.clone(),
                        });
                    }
                    if sess.ip_address != row.ip_address {
                        issues.push(DiffIssue {
                            domain: "auth_sessions".into(),
                            id: row.jti.clone(),
                            field: "ip_address".into(),
                            sql_value: row
                                .ip_address
                                .clone()
                                .unwrap_or_else(|| "None".into()),
                            kv_value: sess
                                .ip_address
                                .clone()
                                .unwrap_or_else(|| "None".into()),
                        });
                    }
                    if sess.user_agent != row.user_agent {
                        issues.push(DiffIssue {
                            domain: "auth_sessions".into(),
                            id: row.jti.clone(),
                            field: "user_agent".into(),
                            sql_value: row
                                .user_agent
                                .clone()
                                .unwrap_or_else(|| "None".into()),
                            kv_value: sess
                                .user_agent
                                .clone()
                                .unwrap_or_else(|| "None".into()),
                        });
                    }
                    if sess.expires_at != row.expires_at {
                        issues.push(DiffIssue {
                            domain: "auth_sessions".into(),
                            id: row.jti.clone(),
                            field: "expires_at".into(),
                            sql_value: row.expires_at.to_string(),
                            kv_value: sess.expires_at.to_string(),
                        });
                    }
                }
                None => issues.push(DiffIssue {
                    domain: "auth_sessions".into(),
                    id: row.jti.clone(),
                    field: "_existence".into(),
                    sql_value: "present".into(),
                    kv_value: "missing".into(),
                }),
            }
        }

        Ok(issues)
    }

    pub async fn diff_runtime_sessions(&self) -> adapteros_core::Result<Vec<DiffIssue>> {
        let mut issues = Vec::new();
        let Some(pool) = self.pool_opt() else {
            return Ok(issues);
        };

        #[derive(sqlx::FromRow)]
        struct RuntimeRow {
            id: String,
            session_id: String,
            config_hash: String,
            binary_version: String,
            binary_commit: Option<String>,
            started_at: String,
            ended_at: Option<String>,
            end_reason: Option<String>,
            hostname: String,
            runtime_mode: String,
            drift_detected: i64,
        }

        let mut rows = sqlx::query_as::<_, RuntimeRow>(
            r#"
            SELECT id, session_id, config_hash, binary_version, binary_commit, started_at, ended_at,
                   end_reason, hostname, runtime_mode, drift_detected
            FROM runtime_sessions
            ORDER BY started_at DESC, id ASC
            "#,
        )
        .fetch_all(pool)
        .await?;

        rows.sort_by(|a, b| {
            b.started_at
                .cmp(&a.started_at)
                .then_with(|| a.id.cmp(&b.id))
        });

        let Some(kv) = self.kv_backend() else {
            return Ok(issues);
        };
        let repo = RuntimeSessionKvRepository::new(kv.backend().clone());

        for row in rows {
            let kv_session = repo.get(&row.id).await?;
            match kv_session {
                Some(sess) => {
                    if sess.session_id != row.session_id {
                        issues.push(DiffIssue {
                            domain: "runtime_sessions".into(),
                            id: row.id.clone(),
                            field: "session_id".into(),
                            sql_value: row.session_id.clone(),
                            kv_value: sess.session_id.clone(),
                        });
                    }
                    if sess.config_hash != row.config_hash {
                        issues.push(DiffIssue {
                            domain: "runtime_sessions".into(),
                            id: row.id.clone(),
                            field: "config_hash".into(),
                            sql_value: row.config_hash.clone(),
                            kv_value: sess.config_hash.clone(),
                        });
                    }
                    if sess.binary_version != row.binary_version {
                        issues.push(DiffIssue {
                            domain: "runtime_sessions".into(),
                            id: row.id.clone(),
                            field: "binary_version".into(),
                            sql_value: row.binary_version.clone(),
                            kv_value: sess.binary_version.clone(),
                        });
                    }
                    if sess.binary_commit != row.binary_commit {
                        issues.push(DiffIssue {
                            domain: "runtime_sessions".into(),
                            id: row.id.clone(),
                            field: "binary_commit".into(),
                            sql_value: row
                                .binary_commit
                                .clone()
                                .unwrap_or_else(|| "None".into()),
                            kv_value: sess
                                .binary_commit
                                .clone()
                                .unwrap_or_else(|| "None".into()),
                        });
                    }
                    if sess.hostname != row.hostname {
                        issues.push(DiffIssue {
                            domain: "runtime_sessions".into(),
                            id: row.id.clone(),
                            field: "hostname".into(),
                            sql_value: row.hostname.clone(),
                            kv_value: sess.hostname.clone(),
                        });
                    }
                    if sess.runtime_mode != row.runtime_mode {
                        issues.push(DiffIssue {
                            domain: "runtime_sessions".into(),
                            id: row.id.clone(),
                            field: "runtime_mode".into(),
                            sql_value: row.runtime_mode.clone(),
                            kv_value: sess.runtime_mode.clone(),
                        });
                    }
                    if sess.drift_detected != (row.drift_detected != 0) {
                        issues.push(DiffIssue {
                            domain: "runtime_sessions".into(),
                            id: row.id.clone(),
                            field: "drift_detected".into(),
                            sql_value: format!("{}", row.drift_detected != 0),
                            kv_value: format!("{}", sess.drift_detected),
                        });
                    }
                }
                None => issues.push(DiffIssue {
                    domain: "runtime_sessions".into(),
                    id: row.id.clone(),
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
        issues.extend(self.diff_auth_sessions().await?);
        issues.extend(self.diff_runtime_sessions().await?);
        issues.extend(self.diff_documents().await?);
        issues.extend(self.diff_collections().await?);
        issues.extend(self.diff_collection_links().await?);
        issues.extend(self.diff_policy_audit().await?);
        issues.extend(self.diff_training_jobs().await?);
        issues.extend(self.diff_chat_sessions().await?);
        Ok(issues)
    }
}
