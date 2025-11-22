# Template Backend Implementation Checklist

## Overview
This checklist tracks the implementation of the prompt template backend API.
See `ANALYSIS_TEMPLATE_BACKEND_DECISION.md` for the full analysis and design.

---

## Phase 1: Database Schema

- [ ] Create migration file `NNNN_prompt_templates.sql`
  - [ ] `templates` table (id, tenant_id, created_by, updated_by, name, description, content, category, variables_json, is_public, is_built_in, version_number, created_at, updated_at)
  - [ ] Unique constraint: (tenant_id, name)
  - [ ] Indexes: idx_templates_tenant_id, idx_templates_category, idx_templates_created_by, idx_templates_is_public

- [ ] Create migration file `NNNN_template_sharing.sql`
  - [ ] `template_sharing` table (id, template_id, shared_by, shared_with_user_id, permission, created_at)
  - [ ] Unique constraint: (template_id, shared_with_user_id)
  - [ ] Indexes: idx_template_sharing_template_id, idx_template_sharing_user_id

- [ ] Create migration file `NNNN_template_usage_logs.sql`
  - [ ] `template_usage_logs` table (id, template_id, user_id, action, changes_json, created_at)
  - [ ] Indexes: idx_template_usage_logs_template_id, idx_template_usage_logs_user_id

- [ ] Sign migrations with `./scripts/sign_migrations.sh`

- [ ] Test migrations: `cargo test -p adapteros-db schema_consistency_tests`

---

## Phase 2: Database Abstraction Layer

- [ ] Create `crates/adapteros-db/src/templates.rs`
  - [ ] `TemplateRecord` struct
  - [ ] `TemplateSharingRecord` struct
  - [ ] `TemplateUsageLog` struct

- [ ] Implement database methods in `Db` trait:
  - [ ] `create_template(&self, req: CreateTemplateRequest) -> Result<TemplateRecord>`
  - [ ] `get_template(&self, id: &str, tenant_id: &str) -> Result<TemplateRecord>`
  - [ ] `list_templates(&self, tenant_id: &str, filter: ListFilter) -> Result<Vec<TemplateRecord>>`
  - [ ] `update_template(&self, id: &str, tenant_id: &str, updates: UpdateTemplateRequest) -> Result<TemplateRecord>`
  - [ ] `delete_template(&self, id: &str, tenant_id: &str) -> Result<()>`
  - [ ] `share_template(&self, template_id: &str, shared_with_user_id: &str, permission: &str) -> Result<()>`
  - [ ] `log_template_usage(&self, template_id: &str, user_id: &str, action: &str) -> Result<()>`

- [ ] Add export to `crates/adapteros-db/src/lib.rs`

---

## Phase 3: Permissions

- [ ] Update `crates/adapteros-server-api/src/permissions.rs`
  - [ ] Add `TemplateView` permission
  - [ ] Add `TemplateCreate` permission
  - [ ] Add `TemplateEdit` permission
  - [ ] Add `TemplateDelete` permission
  - [ ] Add `TemplateShare` permission

- [ ] Update permission matrix in `has_permission()`:
  - [ ] Admin: All template permissions
  - [ ] Operator: View, Create, Edit (own), Share (own)
  - [ ] SRE: View, audit access
  - [ ] Compliance: View only
  - [ ] Viewer: View public only

---

## Phase 4: REST API Handlers

- [ ] Create `crates/adapteros-server-api/src/handlers/templates.rs`

- [ ] Implement handlers:
  - [ ] `list_templates()` - GET /v1/templates
  - [ ] `create_template()` - POST /v1/templates
  - [ ] `get_template()` - GET /v1/templates/:id
  - [ ] `update_template()` - PUT /v1/templates/:id
  - [ ] `delete_template()` - DELETE /v1/templates/:id
  - [ ] `share_template()` - POST /v1/templates/:id/share
  - [ ] `export_templates()` - GET /v1/templates/export
  - [ ] `import_templates()` - POST /v1/templates/import
  - [ ] `get_categories()` - GET /v1/templates/categories
  - [ ] `get_template_usage()` - GET /v1/templates/:id/usage

- [ ] Add permission checks to all handlers:
  - [ ] Check `require_permission()` at start of each handler
  - [ ] Include tenant_id validation (use Claims.tenant_id)

- [ ] Add audit logging:
  - [ ] Call `log_success()` or `log_failure()` for each operation
  - [ ] Include template_id in resource field

- [ ] Add proper error handling:
  - [ ] 404 for not found
  - [ ] 403 for forbidden
  - [ ] 400 for validation errors
  - [ ] 500 for server errors

- [ ] Document with OpenAPI/utoipa attributes

---

## Phase 5: Router Integration

- [ ] Update `crates/adapteros-server-api/src/routes.rs`
  - [ ] Add templates router
  - [ ] Mount at `/v1/templates`

- [ ] Update `crates/adapteros-server-api/src/handlers.rs`
  - [ ] Export templates handlers

---

## Phase 6: Frontend API Client

- [ ] Update `ui/src/api/client.ts`
  - [ ] Add `getTemplates(params)` function
  - [ ] Add `createTemplate(req)` function
  - [ ] Add `getTemplate(id)` function
  - [ ] Add `updateTemplate(id, updates)` function
  - [ ] Add `deleteTemplate(id)` function
  - [ ] Add `shareTemplate(id, req)` function
  - [ ] Add `exportTemplates(ids)` function
  - [ ] Add `importTemplates(file)` function
  - [ ] Add `getTemplateCategories()` function

---

## Phase 7: Frontend Hook Update

- [ ] Update `ui/src/hooks/usePromptTemplates.ts`
  - [ ] Replace localStorage getItem with `client.getTemplates()`
  - [ ] Replace localStorage setItem with `client.createTemplate()`
  - [ ] Update `createTemplate()` to call API
  - [ ] Update `updateTemplate()` to call API
  - [ ] Update `deleteTemplate()` to call API
  - [ ] Remove localStorage save calls
  - [ ] Add loading states for API calls
  - [ ] Add error handling
  - [ ] Optional: Add caching layer with IndexedDB

- [ ] Test hook with backend:
  - [ ] Create, read, update, delete operations
  - [ ] Error handling
  - [ ] Permission enforcement

---

## Phase 8: Frontend Component Updates

- [ ] Update `ui/src/components/PromptTemplateManager.tsx`
  - [ ] Add permission checks (disable buttons if not allowed)
  - [ ] Add sharing UI
  - [ ] Show "locked" indicator for read-only templates
  - [ ] Update delete handler to show "Cannot delete" if permission denied

- [ ] Update `ui/src/components/inference/PromptTemplateManagerNew.tsx`
  - [ ] Same permission checks as above

- [ ] Test with different roles:
  - [ ] Admin - Can do everything
  - [ ] Operator - Can't delete, limited share
  - [ ] Compliance - Read-only
  - [ ] Viewer - Read-only public

---

## Phase 9: Built-in Templates Seeding

- [ ] Create database seeding function
  - [ ] Initialize with 10 built-in templates
  - [ ] Mark as `is_built_in = 1`
  - [ ] Run during first migration

- [ ] Verify templates appear in UI without user action

---

## Phase 10: Data Migration

- [ ] Create migration tool
  - [ ] Export localStorage to JSON format
  - [ ] Call `/v1/templates/import` endpoint
  - [ ] Verify count matches
  - [ ] Display success/error messages

- [ ] Add migration UI button (optional):
  - [ ] "Migrate from Browser Storage" button
  - [ ] Show progress
  - [ ] Show before/after counts

---

## Phase 11: Testing

- [ ] Unit tests:
  - [ ] Database CRUD functions
  - [ ] Permission checks
  - [ ] Variable detection
  - [ ] Template validation

- [ ] Integration tests:
  - [ ] API endpoint tests
  - [ ] Request/response validation
  - [ ] Permission enforcement
  - [ ] Tenant isolation
  - [ ] Audit logging

- [ ] Frontend tests:
  - [ ] Hook functionality
  - [ ] Component rendering
  - [ ] API error handling
  - [ ] Permission-based UI rendering

---

## Phase 12: Documentation

- [ ] Update CLAUDE.md with:
  - [ ] Template API endpoints
  - [ ] Permission matrix for templates
  - [ ] Usage example

- [ ] Add OpenAPI documentation:
  - [ ] Generate with `cargo run --bin export-openapi`
  - [ ] Verify all endpoints documented

- [ ] Add example requests/responses

---

## Phase 13: Rollout

- [ ] Feature flag (optional):
  - [ ] Add `enable_template_backend` flag
  - [ ] Gradual rollout to users

- [ ] Monitoring:
  - [ ] Add metrics for API calls
  - [ ] Monitor error rates
  - [ ] Track usage

- [ ] Announce to users:
  - [ ] Document new sharing capabilities
  - [ ] Explain how to migrate data
  - [ ] Show new permission model

---

## Risks & Mitigations

| Risk | Mitigation |
|------|-----------|
| Data loss during migration | Backup localStorage, verify import count matches export |
| Performance issues | Add database indexes, implement pagination |
| Permission bypass | Comprehensive permission tests before release |
| Audit log bloat | Archive old logs, implement retention policy |
| Backward compatibility | Dual-read mode during transition period |

---

## Success Criteria

- [x] Analysis complete and documented
- [ ] Database schema passes migration tests
- [ ] All CRUD handlers implement required permissions
- [ ] Frontend hook replaced localStorage with API calls
- [ ] Permission checks prevent unauthorized access
- [ ] Audit logs show all template operations
- [ ] Built-in templates available on first login
- [ ] Data migration tool works without data loss
- [ ] All tests passing (unit + integration + frontend)
- [ ] Documentation updated
- [ ] Rollout complete with zero data loss

---

**Status:** Phase 1 (Analysis Complete)
**Next Action:** Begin database schema implementation
**Estimated Duration:** 2-3 implementation steps
