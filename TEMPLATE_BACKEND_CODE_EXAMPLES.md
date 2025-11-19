# Template Backend - Code Examples

Reference implementations showing how to structure the backend code following AdapterOS patterns.

---

## 1. Database Abstraction Layer

### File: `crates/adapteros-db/src/templates.rs`

```rust
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sqlx::{Row, Sqlite, Transaction};
use crate::Result;

/// Template record from database
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TemplateRecord {
    pub id: String,
    pub tenant_id: String,
    pub created_by: String,
    pub updated_by: String,
    pub name: String,
    pub description: String,
    pub content: String,
    pub category: String,
    pub variables: Vec<String>,
    pub is_public: bool,
    pub is_built_in: bool,
    pub version_number: u32,
    pub created_at: String,
    pub updated_at: String,
}

/// Request to create a new template
#[derive(Debug, Deserialize)]
pub struct CreateTemplateRequest {
    pub name: String,
    pub description: String,
    pub content: String,
    pub category: String,
    pub is_public: Option<bool>,
}

/// Request to update a template
#[derive(Debug, Deserialize)]
pub struct UpdateTemplateRequest {
    pub name: Option<String>,
    pub description: Option<String>,
    pub content: Option<String>,
    pub category: Option<String>,
    pub is_public: Option<bool>,
}

/// Template list filter
#[derive(Debug, Clone)]
pub struct TemplateListFilter {
    pub category: Option<String>,
    pub search: Option<String>,
    pub is_public: Option<bool>,
    pub limit: i64,
    pub offset: i64,
}

/// Sharing permission
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SharePermission {
    View,
    Edit,
}

impl SharePermission {
    pub fn as_str(&self) -> &str {
        match self {
            SharePermission::View => "view",
            SharePermission::Edit => "edit",
        }
    }

    pub fn from_str(s: &str) -> Option<Self> {
        match s {
            "view" => Some(SharePermission::View),
            "edit" => Some(SharePermission::Edit),
            _ => None,
        }
    }
}

/// Extract variables from template content
pub fn extract_variables(content: &str) -> Vec<String> {
    let mut variables = std::collections::HashSet::new();
    let re = regex::Regex::new(r"\{\{(\w+)\}\}").unwrap();

    for caps in re.captures_iter(content) {
        if let Some(var) = caps.get(1) {
            variables.insert(var.as_str().to_string());
        }
    }

    let mut result: Vec<_> = variables.into_iter().collect();
    result.sort();
    result
}

/// Database operations for templates
pub trait TemplateOps: Send + Sync {
    /// Create a new template
    async fn create_template(
        &self,
        tenant_id: &str,
        created_by: &str,
        req: &CreateTemplateRequest,
    ) -> Result<TemplateRecord>;

    /// Get a single template by ID
    async fn get_template(&self, id: &str, tenant_id: &str) -> Result<TemplateRecord>;

    /// List templates for a tenant
    async fn list_templates(&self, tenant_id: &str, filter: &TemplateListFilter) -> Result<Vec<TemplateRecord>>;

    /// Count total templates matching filter
    async fn count_templates(&self, tenant_id: &str, filter: &TemplateListFilter) -> Result<i64>;

    /// Update a template
    async fn update_template(
        &self,
        id: &str,
        tenant_id: &str,
        updated_by: &str,
        req: &UpdateTemplateRequest,
    ) -> Result<TemplateRecord>;

    /// Delete a template
    async fn delete_template(&self, id: &str, tenant_id: &str) -> Result<()>;

    /// Share a template with another user
    async fn share_template(
        &self,
        template_id: &str,
        tenant_id: &str,
        shared_by: &str,
        shared_with_user_id: &str,
        permission: SharePermission,
    ) -> Result<()>;

    /// Get sharing information for a template
    async fn get_sharing(
        &self,
        template_id: &str,
        tenant_id: &str,
    ) -> Result<Vec<(String, SharePermission)>>;

    /// Check if user has permission to access template
    async fn check_template_access(
        &self,
        template_id: &str,
        tenant_id: &str,
        user_id: &str,
        required_permission: SharePermission,
    ) -> Result<bool>;

    /// Log template usage
    async fn log_template_usage(
        &self,
        template_id: &str,
        user_id: &str,
        action: &str,
    ) -> Result<()>;

    /// Get template usage statistics
    async fn get_template_usage(
        &self,
        template_id: &str,
        tenant_id: &str,
    ) -> Result<(u64, DateTime<Utc>, Option<DateTime<Utc>>)>;
}
```

---

## 2. REST Handler

### File: `crates/adapteros-server-api/src/handlers/templates.rs`

```rust
use crate::auth::Claims;
use crate::permissions::{require_permission, Permission};
use crate::audit_helper::{log_success, log_failure, actions, resources};
use crate::state::AppState;
use crate::types::*;
use adapteros_db::{TemplateOps, SharePermission, TemplateListFilter, CreateTemplateRequest, UpdateTemplateRequest};
use axum::{
    extract::{Path, Query, State},
    http::StatusCode,
    Extension, Json,
};
use serde::{Deserialize, Serialize};
use tracing::{error, info, warn};

/// List templates for current tenant
#[utoipa::path(
    get,
    path = "/v1/templates",
    params(
        ("category", description = "Filter by category"),
        ("search", description = "Search by name or description"),
        ("is_public", description = "Filter public templates"),
        ("limit", description = "Results per page"),
        ("offset", description = "Pagination offset"),
    ),
    responses(
        (status = 200, description = "List of templates", body = TemplateListResponse),
        (status = 400, description = "Invalid filter"),
        (status = 403, description = "Permission denied"),
    )
)]
pub async fn list_templates(
    State(state): State<AppState>,
    Extension(claims): Extension<Claims>,
    Query(params): Query<ListTemplatesParams>,
) -> Result<Json<TemplateListResponse>, ErrorResponse> {
    // Check permission
    require_permission(&claims, Permission::TemplateView)?;

    let filter = TemplateListFilter {
        category: params.category,
        search: params.search,
        is_public: params.is_public,
        limit: params.limit.unwrap_or(50).min(100),
        offset: params.offset.unwrap_or(0),
    };

    // Query database with tenant isolation
    let templates = state
        .db
        .list_templates(&claims.tenant_id, &filter)
        .await
        .map_err(|e| {
            error!(error = %e, "Failed to list templates");
            ErrorResponse::new(StatusCode::INTERNAL_SERVER_ERROR, "Failed to list templates")
        })?;

    let total = state
        .db
        .count_templates(&claims.tenant_id, &filter)
        .await
        .unwrap_or(0);

    info!(
        tenant_id = %claims.tenant_id,
        user_id = %claims.user_id,
        count = templates.len(),
        "Listed templates"
    );

    Ok(Json(TemplateListResponse {
        total,
        offset: filter.offset,
        limit: filter.limit,
        templates: templates.into_iter().map(|t| t.into()).collect(),
    }))
}

/// Create a new template
#[utoipa::path(
    post,
    path = "/v1/templates",
    request_body = CreateTemplateRequest,
    responses(
        (status = 201, description = "Template created", body = TemplateResponse),
        (status = 400, description = "Invalid request"),
        (status = 403, description = "Permission denied"),
    )
)]
pub async fn create_template(
    State(state): State<AppState>,
    Extension(claims): Extension<Claims>,
    Json(req): Json<CreateTemplateRequest>,
) -> Result<(StatusCode, Json<TemplateResponse>), ErrorResponse> {
    // Check permission
    require_permission(&claims, Permission::TemplateCreate)?;

    // Validate request
    if req.name.trim().is_empty() {
        return Err(ErrorResponse::new(
            StatusCode::BAD_REQUEST,
            "Template name is required",
        ));
    }
    if req.content.trim().is_empty() {
        return Err(ErrorResponse::new(
            StatusCode::BAD_REQUEST,
            "Template content is required",
        ));
    }

    // Create in database
    let template = state
        .db
        .create_template(&claims.tenant_id, &claims.user_id, &req)
        .await
        .map_err(|e| {
            error!(error = %e, "Failed to create template");
            ErrorResponse::new(StatusCode::INTERNAL_SERVER_ERROR, "Failed to create template")
        })?;

    // Log audit
    let _ = log_success(
        &state.db,
        &claims,
        actions::TEMPLATE_CREATE,
        resources::TEMPLATE,
        Some(&template.id),
    )
    .await;

    info!(
        tenant_id = %claims.tenant_id,
        user_id = %claims.user_id,
        template_id = %template.id,
        "Created template"
    );

    Ok((StatusCode::CREATED, Json(template.into())))
}

/// Get a single template
#[utoipa::path(
    get,
    path = "/v1/templates/{id}",
    params(("id", description = "Template ID")),
    responses(
        (status = 200, description = "Template detail", body = TemplateResponse),
        (status = 404, description = "Not found"),
        (status = 403, description = "Permission denied"),
    )
)]
pub async fn get_template(
    State(state): State<AppState>,
    Extension(claims): Extension<Claims>,
    Path(id): Path<String>,
) -> Result<Json<TemplateResponse>, ErrorResponse> {
    // Check permission
    require_permission(&claims, Permission::TemplateView)?;

    let template = state
        .db
        .get_template(&id, &claims.tenant_id)
        .await
        .map_err(|_| {
            ErrorResponse::new(StatusCode::NOT_FOUND, "Template not found")
        })?;

    // Log usage
    let _ = state.db.log_template_usage(&id, &claims.user_id, "view").await;

    Ok(Json(template.into()))
}

/// Update a template
#[utoipa::path(
    put,
    path = "/v1/templates/{id}",
    request_body = UpdateTemplateRequest,
    params(("id", description = "Template ID")),
    responses(
        (status = 200, description = "Template updated", body = TemplateResponse),
        (status = 404, description = "Not found"),
        (status = 403, description = "Permission denied"),
    )
)]
pub async fn update_template(
    State(state): State<AppState>,
    Extension(claims): Extension<Claims>,
    Path(id): Path<String>,
    Json(req): Json<UpdateTemplateRequest>,
) -> Result<Json<TemplateResponse>, ErrorResponse> {
    // Check permission
    require_permission(&claims, Permission::TemplateEdit)?;

    // Get current template to check ownership
    let current = state
        .db
        .get_template(&id, &claims.tenant_id)
        .await
        .map_err(|_| {
            ErrorResponse::new(StatusCode::NOT_FOUND, "Template not found")
        })?;

    // Check if user can edit (owner or admin)
    if current.created_by != claims.user_id && !claims.is_admin() {
        return Err(ErrorResponse::new(
            StatusCode::FORBIDDEN,
            "You can only edit your own templates",
        ));
    }

    let template = state
        .db
        .update_template(&id, &claims.tenant_id, &claims.user_id, &req)
        .await
        .map_err(|e| {
            error!(error = %e, "Failed to update template");
            ErrorResponse::new(StatusCode::INTERNAL_SERVER_ERROR, "Failed to update template")
        })?;

    // Log audit
    let _ = log_success(
        &state.db,
        &claims,
        actions::TEMPLATE_UPDATE,
        resources::TEMPLATE,
        Some(&id),
    )
    .await;

    info!(
        tenant_id = %claims.tenant_id,
        user_id = %claims.user_id,
        template_id = %id,
        "Updated template"
    );

    Ok(Json(template.into()))
}

/// Delete a template
#[utoipa::path(
    delete,
    path = "/v1/templates/{id}",
    params(("id", description = "Template ID")),
    responses(
        (status = 204, description = "Template deleted"),
        (status = 404, description = "Not found"),
        (status = 403, description = "Permission denied"),
    )
)]
pub async fn delete_template(
    State(state): State<AppState>,
    Extension(claims): Extension<Claims>,
    Path(id): Path<String>,
) -> Result<StatusCode, ErrorResponse> {
    // Check permission
    require_permission(&claims, Permission::TemplateDelete)?;

    // Get template to check ownership
    let template = state
        .db
        .get_template(&id, &claims.tenant_id)
        .await
        .map_err(|_| {
            ErrorResponse::new(StatusCode::NOT_FOUND, "Template not found")
        })?;

    // Prevent deletion of built-in templates
    if template.is_built_in {
        return Err(ErrorResponse::new(
            StatusCode::FORBIDDEN,
            "Cannot delete built-in templates",
        ));
    }

    // Only admin can delete templates
    if !claims.is_admin() {
        return Err(ErrorResponse::new(
            StatusCode::FORBIDDEN,
            "Only admins can delete templates",
        ));
    }

    state
        .db
        .delete_template(&id, &claims.tenant_id)
        .await
        .map_err(|e| {
            error!(error = %e, "Failed to delete template");
            ErrorResponse::new(StatusCode::INTERNAL_SERVER_ERROR, "Failed to delete template")
        })?;

    // Log audit
    let _ = log_success(
        &state.db,
        &claims,
        actions::TEMPLATE_DELETE,
        resources::TEMPLATE,
        Some(&id),
    )
    .await;

    info!(
        tenant_id = %claims.tenant_id,
        user_id = %claims.user_id,
        template_id = %id,
        "Deleted template"
    );

    Ok(StatusCode::NO_CONTENT)
}

/// Share a template with another user
#[utoipa::path(
    post,
    path = "/v1/templates/{id}/share",
    request_body = ShareTemplateRequest,
    params(("id", description = "Template ID")),
    responses(
        (status = 200, description = "Template shared"),
        (status = 404, description = "Not found"),
        (status = 403, description = "Permission denied"),
    )
)]
pub async fn share_template(
    State(state): State<AppState>,
    Extension(claims): Extension<Claims>,
    Path(id): Path<String>,
    Json(req): Json<ShareTemplateRequest>,
) -> Result<Json<serde_json::Value>, ErrorResponse> {
    // Check permission
    require_permission(&claims, Permission::TemplateShare)?;

    // Verify template exists and belongs to tenant
    state
        .db
        .get_template(&id, &claims.tenant_id)
        .await
        .map_err(|_| {
            ErrorResponse::new(StatusCode::NOT_FOUND, "Template not found")
        })?;

    // Share with each user
    for user_id in &req.user_ids {
        let permission = SharePermission::from_str(&req.permission)
            .ok_or_else(|| {
                ErrorResponse::new(StatusCode::BAD_REQUEST, "Invalid permission level")
            })?;

        state
            .db
            .share_template(&id, &claims.tenant_id, &claims.user_id, user_id, permission)
            .await
            .map_err(|e| {
                error!(error = %e, "Failed to share template");
                ErrorResponse::new(StatusCode::INTERNAL_SERVER_ERROR, "Failed to share template")
            })?;
    }

    // Log audit
    let _ = log_success(
        &state.db,
        &claims,
        actions::TEMPLATE_SHARE,
        resources::TEMPLATE,
        Some(&id),
    )
    .await;

    info!(
        tenant_id = %claims.tenant_id,
        user_id = %claims.user_id,
        template_id = %id,
        shared_with_count = req.user_ids.len(),
        "Shared template"
    );

    Ok(Json(serde_json::json!({
        "status": "shared",
        "template_id": id,
        "shared_with_count": req.user_ids.len(),
    })))
}

/// Request models
#[derive(Debug, Deserialize)]
pub struct ListTemplatesParams {
    pub category: Option<String>,
    pub search: Option<String>,
    pub is_public: Option<bool>,
    pub limit: Option<i64>,
    pub offset: Option<i64>,
}

#[derive(Debug, Deserialize)]
pub struct ShareTemplateRequest {
    pub user_ids: Vec<String>,
    pub permission: String,  // "view" or "edit"
}

/// Response models
#[derive(Debug, Serialize)]
pub struct TemplateResponse {
    pub id: String,
    pub name: String,
    pub description: String,
    pub content: String,
    pub category: String,
    pub variables: Vec<String>,
    pub is_public: bool,
    pub is_built_in: bool,
    pub version_number: u32,
    pub created_at: String,
    pub updated_at: String,
    pub created_by: String,
}

#[derive(Debug, Serialize)]
pub struct TemplateListResponse {
    pub total: i64,
    pub offset: i64,
    pub limit: i64,
    pub templates: Vec<TemplateResponse>,
}
```

---

## 3. Frontend Hook Update

### File: `ui/src/hooks/usePromptTemplates.ts` (Updated)

```typescript
import { useState, useCallback, useEffect } from 'react';
import { client } from '../api/client';
import { PromptTemplate } from './types';

export interface UsePromptTemplatesOptions {
  tenantId?: string;
  onError?: (error: Error) => void;
}

export function usePromptTemplates(options?: UsePromptTemplatesOptions) {
  const [templates, setTemplates] = useState<PromptTemplate[]>([]);
  const [isLoading, setIsLoading] = useState(true);
  const [error, setError] = useState<string | null>(null);

  // Load templates from server on mount
  useEffect(() => {
    const loadTemplates = async () => {
      try {
        setIsLoading(true);
        const response = await client.getTemplates({
          limit: 100,
          offset: 0,
        });
        setTemplates(response.templates || []);
        setError(null);
      } catch (err) {
        const message = err instanceof Error ? err.message : 'Failed to load templates';
        setError(message);
        options?.onError?.(new Error(message));
        // Keep using any cached templates on error
      } finally {
        setIsLoading(false);
      }
    };

    loadTemplates();
  }, []);

  // Create new template
  const createTemplate = useCallback(
    async (name: string, description: string, prompt: string, category: string) => {
      try {
        const newTemplate = await client.createTemplate({
          name,
          description,
          content: prompt,
          category,
          is_public: false,
        });

        setTemplates(prev => [...prev, newTemplate]);
        return newTemplate;
      } catch (err) {
        const message = err instanceof Error ? err.message : 'Failed to create template';
        setError(message);
        throw err;
      }
    },
    []
  );

  // Update template
  const updateTemplate = useCallback(
    async (id: string, updates: Partial<PromptTemplate>) => {
      try {
        const updated = await client.updateTemplate(id, {
          name: updates.name,
          description: updates.description,
          content: updates.prompt,
          category: updates.category,
          is_public: updates.isFavorite,
        });

        setTemplates(prev =>
          prev.map(t => (t.id === id ? updated : t))
        );
        return updated;
      } catch (err) {
        const message = err instanceof Error ? err.message : 'Failed to update template';
        setError(message);
        throw err;
      }
    },
    []
  );

  // Delete template
  const deleteTemplate = useCallback(
    async (id: string) => {
      try {
        await client.deleteTemplate(id);
        setTemplates(prev => prev.filter(t => t.id !== id));
        return true;
      } catch (err) {
        const message = err instanceof Error ? err.message : 'Failed to delete template';
        setError(message);
        throw err;
      }
    },
    []
  );

  // Rest of methods (getTemplate, getTemplates, etc.) call server API
  // ...

  return {
    templates,
    isLoading,
    error,
    createTemplate,
    updateTemplate,
    deleteTemplate,
    // ... other methods
  };
}
```

---

## 4. Database Migrations

### File: `migrations/NNNN_prompt_templates.sql`

```sql
-- Prompt Templates table
CREATE TABLE IF NOT EXISTS templates (
    id TEXT PRIMARY KEY,
    tenant_id TEXT NOT NULL REFERENCES tenants(id) ON DELETE CASCADE,
    created_by TEXT NOT NULL REFERENCES users(id),
    updated_by TEXT NOT NULL REFERENCES users(id),
    name TEXT NOT NULL,
    description TEXT,
    content TEXT NOT NULL,
    category TEXT NOT NULL CHECK(category IN ('code-review', 'documentation', 'testing', 'debugging', 'refactoring', 'custom')),
    variables_json TEXT NOT NULL DEFAULT '[]',  -- JSON array
    is_public INTEGER NOT NULL DEFAULT 0,
    is_built_in INTEGER NOT NULL DEFAULT 0,
    version_number INTEGER NOT NULL DEFAULT 1,
    created_at TEXT NOT NULL DEFAULT (datetime('now')),
    updated_at TEXT NOT NULL DEFAULT (datetime('now')),
    UNIQUE(tenant_id, name)
);

CREATE INDEX idx_templates_tenant_id ON templates(tenant_id);
CREATE INDEX idx_templates_category ON templates(category);
CREATE INDEX idx_templates_created_by ON templates(created_by);
CREATE INDEX idx_templates_is_public ON templates(is_public);
CREATE INDEX idx_templates_created_at ON templates(created_at DESC);
```

### File: `migrations/NNNN_template_sharing.sql`

```sql
-- Template sharing/permissions
CREATE TABLE IF NOT EXISTS template_sharing (
    id TEXT PRIMARY KEY,
    template_id TEXT NOT NULL REFERENCES templates(id) ON DELETE CASCADE,
    shared_by TEXT NOT NULL REFERENCES users(id),
    shared_with_user_id TEXT NOT NULL REFERENCES users(id),
    permission TEXT NOT NULL CHECK(permission IN ('view', 'edit')),
    created_at TEXT NOT NULL DEFAULT (datetime('now')),
    UNIQUE(template_id, shared_with_user_id)
);

CREATE INDEX idx_template_sharing_template_id ON template_sharing(template_id);
CREATE INDEX idx_template_sharing_user_id ON template_sharing(shared_with_user_id);
```

### File: `migrations/NNNN_template_usage_logs.sql`

```sql
-- Template usage audit trail
CREATE TABLE IF NOT EXISTS template_usage_logs (
    id TEXT PRIMARY KEY,
    template_id TEXT NOT NULL REFERENCES templates(id) ON DELETE CASCADE,
    user_id TEXT NOT NULL REFERENCES users(id),
    action TEXT NOT NULL CHECK(action IN ('create', 'update', 'delete', 'view', 'use', 'share')),
    changes_json TEXT,  -- Track what changed
    created_at TEXT NOT NULL DEFAULT (datetime('now'))
);

CREATE INDEX idx_template_usage_logs_template_id ON template_usage_logs(template_id);
CREATE INDEX idx_template_usage_logs_user_id ON template_usage_logs(user_id);
CREATE INDEX idx_template_usage_logs_created_at ON template_usage_logs(created_at DESC);
```

---

These examples follow the established patterns in the AdapterOS codebase:
- Database trait pattern (see `adapteros-db`)
- Permission checking (see `permissions.rs`)
- Audit logging (see `audit_helper.rs`)
- REST handler structure (see `handlers/adapters.rs`)
- Error handling (see `types.rs`)

---

**Note:** These are template examples. Adjust field names, types, and logic based on final design decisions.
