# ERRORS

Canonical error codes. Source: `adapteros-core/error_codes.rs`.

---

## Response Format

```json
{
  "code": "NOT_FOUND",
  "message": "Adapter not found",
  "details": null
}
```

**Type:** `adapteros_api_types::ErrorResponse`. Handlers use `ApiError::not_found("NOT_FOUND", "message")`.

---

## Categories

```mermaid
flowchart TB
    subgraph 4xx["4xx Client Errors"]
        A["400: BAD_REQUEST, VALIDATION_ERROR<br/>PARSE_ERROR, INVALID_HASH<br/>PREFLIGHT_FAILED, DETERMINISM_ERROR"]
        B["401: UNAUTHORIZED<br/>TOKEN_MISSING, TOKEN_EXPIRED"]
        C["403: FORBIDDEN<br/>PERMISSION_DENIED, POLICY_VIOLATION"]
        D["404: NOT_FOUND<br/>ADAPTER_NOT_FOUND, MODEL_NOT_FOUND"]
        E["409: CONFLICT<br/>ADAPTER_HASH_MISMATCH, DUPLICATE_REQUEST"]
        F["422: REASONING_LOOP_DETECTED"]
        G["429: TOO_MANY_REQUESTS, BACKPRESSURE"]
    end

    subgraph 5xx["5xx Server Errors"]
        H["500: INTERNAL_ERROR<br/>DATABASE_ERROR, CRYPTO_ERROR"]
        I["502: BAD_GATEWAY<br/>NETWORK_ERROR, BASE_LLM_ERROR"]
        J["503: SERVICE_UNAVAILABLE<br/>WORKER_UNAVAILABLE"]
    end

    4xx --> A
    4xx --> B
    4xx --> C
    4xx --> D
    4xx --> E
    4xx --> F
    4xx --> G
    5xx --> H
    5xx --> I
    5xx --> J
```

---

## Key Codes (by domain)

| Domain | Codes |
|--------|-------|
| Validation | BAD_REQUEST, VALIDATION_ERROR, PARSE_ERROR, MISSING_FIELD, INVALID_CPID |
| Auth | UNAUTHORIZED, TOKEN_MISSING, TOKEN_EXPIRED, INVALID_SIGNATURE |
| Policy | FORBIDDEN, PERMISSION_DENIED, POLICY_VIOLATION |
| Resources | NOT_FOUND, ADAPTER_NOT_FOUND, MODEL_NOT_FOUND, WORKER_UNAVAILABLE |
| Determinism | DETERMINISM_ERROR |
| Rate limit | TOO_MANY_REQUESTS, BACKPRESSURE |
| Worker | SERVICE_UNAVAILABLE, WORKER_UNAVAILABLE, BAD_GATEWAY |

---

## Lookup

```bash
./aosctl explain <code>
```

**Source:** `adapteros-cli/src/commands/explain.rs` reads from `adapteros-core` error code registry.
