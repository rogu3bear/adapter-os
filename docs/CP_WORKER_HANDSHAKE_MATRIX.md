# Control Plane ↔ Worker Handshake Failure Matrix

> Comprehensive failure modes for CP-Worker UDS communication.

## System Overview

**Connection Path:** `CP (UdsClient)` → Unix Domain Socket: `var/run/worker.sock` → `Worker (UdsServer)`

**Protocol:** HTTP/1.1 over Unix Domain Socket with optional SSE for streaming

**Key Files:**
- CP Client: `crates/adapteros-server-api/src/uds_client.rs`
- Worker Server: `crates/adapteros-lora-worker/src/uds_server.rs`
- Protocol: `crates/adapteros-uds-protocol/src/`

---

## Authentication Modes

| Mode | Description |
|------|-------------|
| None | Dev mode, no auth |
| ApiKey | Control plane DB validation |
| Bearer JWT | Ed25519 single-key |
| Key Ring | Multi-key with rotation |

---

## Phase 1: Socket Connection

| Failure | Error Code | CP Side | Worker Side | Cause | Recovery |
|---------|------------|---------|-------------|-------|----------|
| Socket Not Found | `ConnectionFailed` | `ENOENT` on connect | N/A | Worker not started, wrong path | Start worker, verify path |
| Connection Refused | `ConnectionFailed` | `ECONNREFUSED` | Not listening | Worker crashed, not started | Restart worker |
| Permission Denied | `ConnectionFailed` | `EACCES` | N/A | Wrong socket permissions | Fix permissions |
| Connection Timeout | `Timeout` | `timeout()` expires | Accept loop blocked | Worker overloaded | Increase timeout |
| Worker Draining | `ConnectionRefused` | Connects but rejected | `drain_flag` set | Graceful shutdown | Retry after restart |

---

## Phase 2: HTTP Request Parsing

| Failure | Error Code | CP Side | Worker Side | Cause | Recovery |
|---------|------------|---------|-------------|-------|----------|
| Serialization Error | `SerializationError` | `to_string()` fails | N/A | Invalid JSON types | Fix payload |
| Request Parse Timeout | `Timeout` | Write succeeds | 5s per-byte timeout | Malformed headers | Resend request |
| Malformed Headers | `RequestFailed` | Invalid HTTP | `parse_request()` fails | CP bug | Fix client |
| Missing Content-Length | `RequestFailed` | POST without header | Can't determine length | CP error | Add header |
| Body Read Timeout | `Timeout` | Headers sent, no body | 30s body read timeout | Network partition | Resend |
| Body Too Large | `RequestFailed` | N/A | Allocation fails | Fraudulent Content-Length | Add size limit |

---

## Phase 3: Authentication

| Failure | Error Code | CP Side | Worker Side | Cause | Recovery |
|---------|------------|---------|-------------|-------|----------|
| Missing Auth Header | `Unauthorized` (401) | No header sent | Auth required | Auth enabled, no header | Add auth header |
| Invalid Scheme | `Unauthorized` (401) | Wrong prefix | `strip_prefix` fails | Wrong auth method | Use correct scheme |
| Token Validation Failed | `Unauthorized` (401) | Bad JWT | `validate_worker_token()` fails | Expired, wrong key | Re-issue token |
| Token Replay | `Unauthorized` (401) | Same JTI reused | JTI cache detects | Replay attack | Use unique JTI |
| Worker ID Mismatch | `Unauthorized` (401) | Wrong `wid` claim | Token for different worker | Misconfiguration | Match worker ID |
| Key Rotation In-Flight | `Unauthorized` (401) | Old key signature | Key ring expired | Grace period too short | Extend grace period |
| API Key Not Found | `Unauthorized` (401) | Hash doesn't match | DB lookup fails | Key revoked | Issue new key |
| API Key Tenant Mismatch | `Unauthorized` (401) | Key valid, wrong tenant | Tenant check fails | Multi-tenant issue | Match tenant |
| JTI Cache Lost | `Unauthorized` (401) | Replay succeeds | Cache persistence failed | Disk write failure | Fix persistence |
| Key Ring Nonce Replay | `Unauthorized` (401) | Old nonce in update | `has_seen_nonce()` detects | Stale update | Fresh nonce |
| Key Update Sig Invalid | `Unauthorized` (401) | Bad signature | `verify_signature()` fails | Wrong signing key | Sign with current key |

---

## Phase 4: HTTP Response Parsing

| Failure | Error Code | CP Side | Worker Side | Cause | Recovery |
|---------|------------|---------|-------------|-------|----------|
| Empty Response | `RequestFailed` | Socket closed | N/A | Worker crash mid-response | Check worker health |
| Non-200 Status | `RequestFailed` | Error status | Intentional error | Inference/resource error | Check error details |
| Malformed JSON | `SerializationError` | `from_str()` fails | N/A | Invalid JSON in response | Retry, add validation |
| Missing Separator | `RequestFailed` | No `\r\n\r\n` | N/A | Non-compliant response | Fix worker format |
| Response Too Large | `RequestFailed` | Unbounded read | N/A | Massive/malicious response | Add size limit |
| 503 Overload | `WorkerOverloaded` | Worker at capacity | Backpressure gate full | Queue full | Backoff, retry |
| Cache Budget Exceeded | `CacheBudgetExceeded` | Model doesn't fit | Budget exceeded | Too many pinned adapters | Unpin adapters |

---

## Phase 5: SSE Streaming

| Failure | Error Code | CP Side | Worker Side | Cause | Recovery |
|---------|------------|---------|-------------|-------|----------|
| Streaming Not Supported | `RequestFailed` | Non-stream response | Wrong request type | Missing headers | Add `X-Stream: true` |
| SSE Status Line Missing | `RequestFailed` | First line unreadable | N/A | Invalid SSE format | Fix worker SSE |
| SSE Header Timeout | `Timeout` | Headers >60s | N/A | Network slow | Increase timeout |
| SSE Line Timeout | `Timeout` | Line >60s | N/A | Review pause or stall | Wait or retry |
| SSE Stream Timeout | `Timeout` | Stream >5min | N/A | Long inference | Increase SSE timeout |
| Invalid SSE Event | `Error(msg)` | Unknown event type | N/A | Protocol mismatch | Update client |
| Incomplete SSE Event | `RequestFailed` | Malformed JSON | N/A | Corruption | Retry |

---

## Phase 6: Circuit Breaker & Backpressure

| Failure | Error Code | CP Side | Worker Side | Cause | Recovery |
|---------|------------|---------|-------------|-------|----------|
| Circuit Breaker Open | `WorkerNotAvailable` | 3+ consecutive failures | N/A | Repeated worker failures | Wait 5s for half-open |
| UDS Accept Failures | Worker shutdown | Can't connect | Accept loop fails 5+ times | Resource exhaustion | Check file descriptors |
| Backpressure Full | `WorkerOverloaded` | Concurrency limit hit | No permits available | Too many requests | Reduce rate |
| Permit Timeout | `Timeout` | Queue wait exceeds limit | N/A | Queue backlog | Increase timeout |

---

## Phase 7: Cancellation

| Failure | Error Code | CP Side | Worker Side | Cause | Recovery |
|---------|------------|---------|-------------|-------|----------|
| Cancel Timeout | `Timeout` | Write/read times out | Handler slow | Worker overloaded | Retry or force |
| Cancel Not Found | `RequestFailed` (404) | Request ID missing | Request completed | Already done | Treat as success |
| Cancel Auth Failure | `Unauthorized` (401) | Rejected | Missing token | No auth header | Include auth |

---

## Phase 8: Network Partition

| Failure | Error Code | CP Side | Worker Side | Cause | Recovery |
|---------|------------|---------|-------------|-------|----------|
| Reset During Write | `RequestFailed` | TCP RST on write | Socket closed | Worker crash, OOM | Check worker logs |
| Reset During Read | `RequestFailed` | Socket closes | Handler panic | Inference error | Check worker logs |
| Partial Request | `RequestFailed` | Connection drops | N/A | Network disconnect | Retry via breaker |
| Partial Response | `RequestFailed` | Incomplete response | Crash mid-response | Worker OOM | Retry |
| Worker Restart | `ConnectionFailed` | Socket invalid | Process restarts | Upgrade/crash | Auto-reconnect |

---

## Phase 9: Determinism & Routing

| Failure | Error Code | CP Side | Worker Side | Cause | Recovery |
|---------|------------|---------|-------------|-------|----------|
| Routing Bypass | `RoutingBypass` | No routing guard | N/A | Direct `infer()` call | Use `InferenceCore` |
| Cancelled Pre-connect | `Cancelled` | Token fires early | N/A | Timeout/explicit cancel | Expected behavior |
| Cancelled Write | `Cancelled` | Token fires mid-write | Cancel sent | Timeout | Cancel processed |
| Cancelled Read | `Cancelled` | Token fires mid-read | Cancel sent | Timeout | May get partial |

---

## Phase 10: Key Rotation

| Failure | Error Code | CP Side | Worker Side | Cause | Recovery |
|---------|------------|---------|-------------|-------|----------|
| Key Update Too Old | `RequestFailed` | Exceeds MAX_AGE_SECS | Rejects stale update | Clock skew, delay | Sync clocks |
| Grace Period Expired | Auth fails | Old key removed early | Premature removal | Calculation error | Extend grace |
| New Key Decode Fails | `RequestFailed` | Malformed key bytes | `decode_new_public_key()` fails | Generation bug | Regenerate key |
| Key Ring Race | Intermittent auth fail | Multiple thread access | Lock contention | High concurrency | Use per-key cache |

---

## Phase 11: Model Loading

| Failure | Error Code | CP Side | Worker Side | Cause | Recovery |
|---------|------------|---------|-------------|-------|----------|
| Model Path Not Found | `RequestFailed` | Invalid path | `exists()` = false | Wrong path, deleted | Verify path |
| Model Load Timeout | `Timeout` | Load takes too long | Stalled validation | Large model, slow storage | Increase timeout |
| Worker Unhealthy After Load | `RequestFailed` | Health check fails | Load succeeded but degraded | Memory pressure | Investigate worker |

---

## Timeout Reference

| Phase | Timeout | Source |
|-------|---------|--------|
| Connection | 30s | `UdsClient::new(timeout)` |
| Per-byte read | 5s | Worker `parse_request()` |
| Request parse | 30s | Worker `handle_connection()` |
| Request body | 30s | Worker `parse_request()` |
| Write operation | 30s | CP client |
| Read operation | 30s | CP client |
| SSE per-line | 60s | CP streaming |
| SSE overall | 300s (5 min) | CP streaming |
| Accept backoff | 100ms → 10s | Worker exponential |
| Circuit breaker reset | 60s | Worker circuit breaker |

---

## Response Formats

**200 OK:**
```
HTTP/1.1 200 OK\r\n
Content-Type: application/json\r\n
Content-Length: {len}\r\n
\r\n
{json_body}
```

**503 Overload:**
```
HTTP/1.1 503 Service Unavailable\r\n
Retry-After: {secs}\r\n
\r\n
{"error":"WORKER_OVERLOADED","retry_after_ms":100}
```

**401 Unauthorized:**
```
HTTP/1.1 401 Unauthorized\r\n
\r\n
{"error":"Unauthorized"}
```

---

## Monitoring Recommendations

**Critical Alerts (page):**
- UDS accept circuit breaker tripped
- 3+ consecutive ConnectionFailed
- Auth failures for valid tokens
- Worker overload >5 min

**Warning Alerts (ticket):**
- Circuit breaker open
- CacheBudgetExceeded errors
- Timeout errors >1%
- Streaming timeout errors

**Observability Metrics:**
- `connect_secs`, `write_secs`, `read_secs`
- Circuit breaker state transitions
- Backpressure permit availability
- Auth method distribution
