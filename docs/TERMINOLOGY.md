# Terminology

AdapterOS uses a single user-facing term across the UI and documentation.

## Canonical Mapping

- **Workspace (UI)**: The name shown to users in the interface, guides, and UI copy.
- **Tenant (DB/internal)**: The internal name for the same concept; database column and internal fields use `tenant_id`.
- **API**: The canonical field name is `tenant_id`. Where supported, `workspace_id` is accepted as an alias for requests, but responses remain `tenant_id`.

## Practical Guidance

- UI components and copy must use **Workspace**.
- Internal identifiers, schema fields, and database columns remain **tenant**.
- When calling the API from UI code, map workspace identifiers to `tenant_id`.
