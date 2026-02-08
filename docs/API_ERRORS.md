# API Error Handling Contract

This document defines the **stable error-response contract** for AdapterOS HTTP APIs.

## Response Shape

All API error responses should be JSON with the following fields (via `adapteros_api_types::ErrorResponse`):

- `message`: human-readable error summary. This **must be safe to display** (no secrets, file paths, raw DB errors).
- `code`: machine-readable failure code. This must be stable enough for clients to branch on.
- `details` (optional): diagnostic context. This may include internal context, but must be **redacted** before leaving the process.

## Producer Rules

- Prefer returning `crate::api_error::ApiError` from handlers (`ApiResult<T>`).
- For internal failures (DB, IO, unexpected invariants):
  - `message` should be generic (e.g. `"database error"`, `"internal error"`).
  - The real cause should go into `details` via `with_redacted_details(...)`.
- For user errors (validation, auth, not found):
  - `message` should be specific and actionable.
  - `details` should be present only when it improves remediation and is safe.

## Middleware Guarantees

`ErrorCodeEnforcementLayer` ensures all **4xx/5xx JSON responses** have a non-empty `code` field.
This is a safety net, not the preferred mechanism: handlers should set meaningful codes directly.

