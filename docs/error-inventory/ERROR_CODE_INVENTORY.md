# ERROR CODE INVENTORY

Generated: 2026-02-19 22:49:08Z

## Summary

- Canonical constants: 126
- Literal emitted via with_code("..."): 180
- Non-canonical literals: 135
- Dynamic emission sites: 32
- Error enums discovered:      102

## Canonical Constants

ADAPTER_BASE_MODEL_MISMATCH
ADAPTER_HASH_MISMATCH
ADAPTER_IN_FLIGHT
ADAPTER_LAYER_HASH_MISMATCH
ADAPTER_NOT_FOUND
ADAPTER_NOT_IN_EFFECTIVE_SET
ADAPTER_NOT_IN_MANIFEST
ADAPTER_NOT_LOADABLE
ADAPTER_NOT_LOADED
ADAPTER_TENANT_MISMATCH
ANOMALY_DETECTED
API_KEY_MODE_NOT_CONFIGURED
BACKPRESSURE
BAD_GATEWAY
BAD_REQUEST
BASE_LLM_ERROR
CACHE_BUDGET_EXCEEDED
CACHE_ENTRY_NOT_FOUND
CACHE_EVICTION
CACHE_STALE
CHAT_TEMPLATE_ERROR
CHECKPOINT_INTEGRITY_FAILED
CIRCUIT_BREAKER_HALF_OPEN
CIRCUIT_BREAKER_OPEN
CLIENT_CLOSED_REQUEST
CONFIG_ERROR
CONFLICT
CPU_THROTTLED
CRYPTO_ERROR
CSRF_ERROR
DATABASE_ERROR
DETERMINISM_ERROR
DETERMINISM_VIOLATION
DEVICE_MISMATCH
DEV_BYPASS_IN_RELEASE
DISK_FULL
DOWNLOAD_FAILED
DUPLICATE_REQUEST
EGRESS_VIOLATION
EVENT_GAP_DETECTED
EXPORT_FAILED
FD_EXHAUSTED
FEATURE_DISABLED
FORBIDDEN
GATEWAY_TIMEOUT
GPU_UNAVAILABLE
HASH_INTEGRITY_FAILURE
HEALTH_CHECK_FAILED
INCOMPATIBLE_BASE_MODEL
INCOMPATIBLE_SCHEMA_VERSION
INSUFFICIENT_ROLE
INTEGRITY_VIOLATION
INTERNAL_ERROR
INVALID_API_KEY
INVALID_AUDIENCE
INVALID_CPID
INVALID_CREDENTIALS
INVALID_HASH
INVALID_ISSUER
INVALID_MANIFEST
INVALID_RESPONSE
INVALID_SEALED_DATA
INVALID_SESSION_ID
INVALID_TENANT_ID
ISOLATION_VIOLATION
JWT_MODE_NOT_CONFIGURED
KERNEL_LAYOUT_MISMATCH
MEMORY_PRESSURE
MFA_REQUIRED
MIGRATION_CHECKSUM_MISMATCH
MIGRATION_FILE_MISSING
MISSING_FIELD
MODEL_ACQUISITION_IN_PROGRESS
MODEL_NOT_FOUND
MODEL_NOT_READY
NETWORK_ERROR
NOT_FOUND
NO_COMPATIBLE_WORKER
OUT_OF_MEMORY
PARSE_ERROR
PAYLOAD_TOO_LARGE
PERFORMANCE_VIOLATION
PERMISSION_DENIED
POLICY_ERROR
POLICY_HASH_MISMATCH
POLICY_VIOLATION
PREFLIGHT_FAILED
PROMOTION_ERROR
RAG_ERROR
RATE_LIMITER_NOT_CONFIGURED
REASONING_LOOP_DETECTED
REPLAY_ERROR
REPO_ALREADY_EXISTS
REPO_ARCHIVED
REPO_NOT_FOUND
REQUEST_TIMEOUT
ROUTING_BYPASS
SCHEMA_VERSION_MISMATCH
SERIALIZATION_ERROR
SERVICE_UNAVAILABLE
SESSION_EXPIRED
SESSION_LOCKED
SIGNATURE_INVALID
SIGNATURE_REQUIRED
SSRF_BLOCKED
STREAM_DISCONNECTED
SYSTEM_QUARANTINED
TEMP_DIR_UNAVAILABLE
TENANT_ISOLATION_ERROR
THREAD_POOL_SATURATED
THUNDERING_HERD_REJECTED
TOKEN_EXPIRED
TOKEN_INVALID
TOKEN_MISSING
TOKEN_REVOKED
TOKEN_SIGNATURE_INVALID
TOO_MANY_REQUESTS
UDS_CONNECTION_FAILED
UNAUTHORIZED
UNSUPPORTED_BACKEND
VALIDATION_ERROR
VERSION_NOT_FOUND
VERSION_NOT_PROMOTABLE
WORKER_DEGRADED
WORKER_ID_UNAVAILABLE
WORKER_NOT_RESPONDING

## Literal Emitted Codes

ACCOUNT_DISABLED
ACCOUNT_LOCKED
ADAPTER_BASE_MODEL_MISMATCH
ADAPTER_FETCH_ERROR
ADAPTER_IN_USE
ADAPTER_NOT_FOUND
ADAPTER_NOT_LOADABLE
ADAPTER_TENANT_MISMATCH
ARCHIVE_FAILED
ATTACH_MODE_MISMATCH
ATTACH_MODE_VIOLATION
AUTH_STATE_ERROR
BACKPRESSURE
BAD_REQUEST
BASE_MODEL_MISMATCH
BOOTSTRAP_ALREADY_COMPLETED
BOOT_EVIDENCE_MISSING
CHUNK_HASH_MISMATCH
CI_NOT_VERIFIED
CLIENT_CLOSED_REQUEST
CONFIG_ERROR
CONFIG_UNAVAILABLE
CONFLICT
COOKIE_ERROR
CRYPTO_ERROR
CSRF_ERROR
DATABASE_ERROR
DATASET_EMPTY
DATASET_ERROR
DATASET_NOT_FOUND
DATASET_TRUST_BLOCKED
DATASET_TRUST_NEEDS_APPROVAL
DATASET_VERSION_NOT_FOUND
DATA_SPEC_HASH_MISMATCH
DB_ERROR
DETERMINISM_ERROR
DEV_BOOTSTRAP_DISABLED
DEV_BYPASS_DISABLED
DRAINING
DUPLICATE_REQUEST
E2E_MODE_DISABLED
EMAIL_EXISTS
EVENT_APPLICATION_FAILED
EXECUTION_FAILED
EXPORT_ERROR
FORBIDDEN
GATES_NOT_READY
HASH_ERROR
HASH_INTEGRITY_FAILURE
HASH_MISMATCH
HOT_SWAP_GATED
INCOMPATIBLE_SCHEMA_VERSION
INTERNAL_ERROR
INTERNAL_SERVER_ERROR
INVALID_ADAPTER_TYPE
INVALID_CHUNK_INDEX
INVALID_CHUNK_SIZE
INVALID_COMMAND
INVALID_CONTRACT_SAMPLE
INVALID_CREDENTIALS
INVALID_DISPLAY_NAME
INVALID_EMAIL
INVALID_EVENT
INVALID_FORMAT
INVALID_HASH
INVALID_HEX
INVALID_JOB_STATUS
INVALID_MFA_CODE
INVALID_PARAMETER
INVALID_PATH
INVALID_PRIORITY
INVALID_REQUEST
INVALID_ROLE
INVALID_SIGNATURE
INVALID_STATE
INVALID_TOKEN
INVALID_TUTORIAL_ID
IO_ERROR
JOB_NOT_COMPLETED
LEGACY_CODE
LEGACY_REPLAY_UNSUPPORTED
LIFECYCLE_DEMOTION_FAILED
LIFECYCLE_DEMOTION_INVALID
LIFECYCLE_ERROR
LIFECYCLE_PROMOTION_FAILED
LIFECYCLE_PROMOTION_INVALID
LIFECYCLE_TRANSITION_DENIED
LINEAGE_LOAD_FAILED
LINEAGE_REQUIRED
MERKLE_ERROR
MFA_CONFIG_ERROR
MFA_NOT_STARTED
MFA_REQUIRED
MFA_SECRET_ERROR
MISSING_CREDENTIALS
MISSING_MFA_VERIFICATION
MISSING_TABLE
MISSING_TENANT_ID
MISSING_TOKEN
MISSING_VERSION
MODEL_COMPATIBILITY_FAILED
MODEL_NOT_READY
NODE_NOT_FOUND
NOT_FOUND
NOT_IMPLEMENTED
NOT_RETRYABLE
NOT_SERVEABLE
NO_COMPATIBLE_WORKER
NO_STACK
NO_TENANT
NO_TRUSTED_KEY
PATH_POLICY_VIOLATION
PATH_TRAVERSAL
PAUSE_INFERENCE_MISMATCH
PAUSE_NOT_FOUND
PAYLOAD_TOO_LARGE
PEER_CREDENTIALS_MISSING
PERMISSION_DENIED
PLUGIN_CHECK_FAILED
PLUGIN_DISABLE_FAILED
PLUGIN_ENABLE_FAILED
PLUGIN_STATUS_FAILED
POLICY_ERROR
POLICY_PACK_CORRUPT
POLICY_UPDATE_FAILED
POLICY_VIOLATION
PREPROCESSING_DISABLED
PREPROCESS_STATUS_ERROR
PRODUCTION_UCRED_REQUIRED
RATE_LIMIT_EXCEEDED
REGISTRATION_DISABLED
REPLAY_ERROR
REPOSITORY_CREATION_FAILED
REPOSITORY_NOT_FOUND
REQUEST_TIMEOUT
ROTATION_MISMATCH
ROUTING_CHAIN_ERROR
ROUTING_ERROR
SERIALIZATION_ERROR
SERVICE_UNAVAILABLE
SESSION_CREATION_ERROR
SESSION_ERROR
SESSION_EXPIRED
SESSION_INVALID
SESSION_LOCKED
SESSION_TOKEN_NOT_SUPPORTED
SIGNATURE_INVALID
SIGNATURE_REQUIRED
SUPERVISOR_NOT_CONFIGURED
TENANT_ACCESS_DENIED
TENANT_HEADER_MISSING
TENANT_ISOLATION_ERROR
TENANT_MISMATCH
TENANT_NOT_FOUND
TESTKIT_ERROR
TESTKIT_PRODUCTION_BLOCKED
TOKEN_ERROR
TOKEN_EXPIRED
TOKEN_GENERATION_ERROR
TOKEN_MISSING
TOKEN_REVOKED
TOKEN_SIGNATURE_INVALID
TRACKER_NOT_AVAILABLE
TRAINING_ERROR
TRAINING_START_FAILED
UNKNOWN_MANIFEST_FIELDS
UNSUPPORTED_BACKEND
UPLOAD_ALREADY_COMPLETE
USER_NOT_FOUND
VALIDATION_ERROR
VERIFICATION_ERROR
VERIFICATION_FAILED
VERSION_CREATION_FAILED
VERSION_NOT_FOUND
WEAK_PASSWORD
WORKER_CAPABILITY_MISSING
WORKER_DEGRADED
WORKER_NOT_FOUND
WORKER_UID_MISMATCH
WORKER_UNAVAILABLE

## Non-Canonical Codes

ACCOUNT_DISABLED
ACCOUNT_LOCKED
ADAPTER_FETCH_ERROR
ADAPTER_IN_USE
ARCHIVE_FAILED
ATTACH_MODE_MISMATCH
ATTACH_MODE_VIOLATION
AUTH_STATE_ERROR
BASE_MODEL_MISMATCH
BOOTSTRAP_ALREADY_COMPLETED
BOOT_EVIDENCE_MISSING
CHUNK_HASH_MISMATCH
CI_NOT_VERIFIED
CONFIG_UNAVAILABLE
COOKIE_ERROR
DATASET_EMPTY
DATASET_ERROR
DATASET_NOT_FOUND
DATASET_TRUST_BLOCKED
DATASET_TRUST_NEEDS_APPROVAL
DATASET_VERSION_NOT_FOUND
DATA_SPEC_HASH_MISMATCH
DB_ERROR
DEV_BOOTSTRAP_DISABLED
DEV_BYPASS_DISABLED
DRAINING
E2E_MODE_DISABLED
EMAIL_EXISTS
EVENT_APPLICATION_FAILED
EXECUTION_FAILED
EXPORT_ERROR
GATES_NOT_READY
HASH_ERROR
HASH_MISMATCH
HOT_SWAP_GATED
INTERNAL_SERVER_ERROR
INVALID_ADAPTER_TYPE
INVALID_CHUNK_INDEX
INVALID_CHUNK_SIZE
INVALID_COMMAND
INVALID_CONTRACT_SAMPLE
INVALID_DISPLAY_NAME
INVALID_EMAIL
INVALID_EVENT
INVALID_FORMAT
INVALID_HEX
INVALID_JOB_STATUS
INVALID_MFA_CODE
INVALID_PARAMETER
INVALID_PATH
INVALID_PRIORITY
INVALID_REQUEST
INVALID_ROLE
INVALID_SIGNATURE
INVALID_STATE
INVALID_TOKEN
INVALID_TUTORIAL_ID
IO_ERROR
JOB_NOT_COMPLETED
LEGACY_CODE
LEGACY_REPLAY_UNSUPPORTED
LIFECYCLE_DEMOTION_FAILED
LIFECYCLE_DEMOTION_INVALID
LIFECYCLE_ERROR
LIFECYCLE_PROMOTION_FAILED
LIFECYCLE_PROMOTION_INVALID
LIFECYCLE_TRANSITION_DENIED
LINEAGE_LOAD_FAILED
LINEAGE_REQUIRED
MERKLE_ERROR
MFA_CONFIG_ERROR
MFA_NOT_STARTED
MFA_SECRET_ERROR
MISSING_CREDENTIALS
MISSING_MFA_VERIFICATION
MISSING_TABLE
MISSING_TENANT_ID
MISSING_TOKEN
MISSING_VERSION
MODEL_COMPATIBILITY_FAILED
NODE_NOT_FOUND
NOT_IMPLEMENTED
NOT_RETRYABLE
NOT_SERVEABLE
NO_STACK
NO_TENANT
NO_TRUSTED_KEY
PATH_POLICY_VIOLATION
PATH_TRAVERSAL
PAUSE_INFERENCE_MISMATCH
PAUSE_NOT_FOUND
PEER_CREDENTIALS_MISSING
PLUGIN_CHECK_FAILED
PLUGIN_DISABLE_FAILED
PLUGIN_ENABLE_FAILED
PLUGIN_STATUS_FAILED
POLICY_PACK_CORRUPT
POLICY_UPDATE_FAILED
PREPROCESSING_DISABLED
PREPROCESS_STATUS_ERROR
PRODUCTION_UCRED_REQUIRED
RATE_LIMIT_EXCEEDED
REGISTRATION_DISABLED
REPOSITORY_CREATION_FAILED
REPOSITORY_NOT_FOUND
ROTATION_MISMATCH
ROUTING_CHAIN_ERROR
ROUTING_ERROR
SESSION_CREATION_ERROR
SESSION_ERROR
SESSION_INVALID
SESSION_TOKEN_NOT_SUPPORTED
SUPERVISOR_NOT_CONFIGURED
TENANT_ACCESS_DENIED
TENANT_HEADER_MISSING
TENANT_MISMATCH
TENANT_NOT_FOUND
TESTKIT_ERROR
TESTKIT_PRODUCTION_BLOCKED
TOKEN_ERROR
TOKEN_GENERATION_ERROR
TRACKER_NOT_AVAILABLE
TRAINING_ERROR
TRAINING_START_FAILED
UNKNOWN_MANIFEST_FIELDS
UPLOAD_ALREADY_COMPLETE
USER_NOT_FOUND
VERIFICATION_ERROR
VERIFICATION_FAILED
VERSION_CREATION_FAILED
WEAK_PASSWORD
WORKER_CAPABILITY_MISSING
WORKER_NOT_FOUND
WORKER_UID_MISMATCH
WORKER_UNAVAILABLE

## Non-Canonical References (file:line)

crates/adapteros-server-api-models/src/handlers.rs:474:                        .with_code("WORKER_UNAVAILABLE")
crates/adapteros-server-api-models/src/handlers.rs:554:                    .with_code("MODEL_COMPATIBILITY_FAILED")
crates/adapteros-server-api-admin/src/handlers/lifecycle.rs:328:                .with_code("SUPERVISOR_NOT_CONFIGURED")
crates/adapteros-server-api-admin/src/handlers/lifecycle.rs:338:    AdminErrorResponse::new(msg.to_string()).with_code("LIFECYCLE_ERROR")
crates/adapteros-server-api-admin/src/handlers/services.rs:103:                    .with_code("SUPERVISOR_NOT_CONFIGURED"),
crates/adapteros-server-api-admin/src/handlers/services.rs:163:                    .with_code("SUPERVISOR_NOT_CONFIGURED"),
crates/adapteros-server-api-admin/src/handlers/services.rs:223:                    .with_code("SUPERVISOR_NOT_CONFIGURED"),
crates/adapteros-server-api-admin/src/handlers/services.rs:278:                    .with_code("SUPERVISOR_NOT_CONFIGURED"),
crates/adapteros-server-api-admin/src/handlers/services.rs:329:                    .with_code("SUPERVISOR_NOT_CONFIGURED"),
crates/adapteros-server-api-admin/src/handlers/services.rs:387:                    .with_code("SUPERVISOR_NOT_CONFIGURED"),
crates/adapteros-server-api-admin/src/handlers/plugins.rs:51:                Json(AdminErrorResponse::new(e.to_string()).with_code("PLUGIN_ENABLE_FAILED")),
crates/adapteros-server-api-admin/src/handlers/plugins.rs:93:                Json(AdminErrorResponse::new(e.to_string()).with_code("PLUGIN_DISABLE_FAILED")),
crates/adapteros-server-api-admin/src/handlers/plugins.rs:132:                Json(AdminErrorResponse::new(e.to_string()).with_code("PLUGIN_STATUS_FAILED")),
crates/adapteros-server-api-admin/src/handlers/plugins.rs:175:            Json(AdminErrorResponse::new(e.to_string()).with_code("DB_ERROR")),
crates/adapteros-server-api-admin/src/handlers/plugins.rs:193:                                    .with_code("PLUGIN_CHECK_FAILED"),
crates/adapteros-server-api-audit/src/handlers.rs:144:                    .with_code("INTERNAL_SERVER_ERROR")
crates/adapteros-server-api-audit/src/handlers.rs:288:                    .with_code("INTERNAL_SERVER_ERROR")
crates/adapteros-server-api-audit/src/handlers.rs:338:                    .with_code("INTERNAL_SERVER_ERROR")
crates/adapteros-server-api-audit/src/handlers.rs:405:                    .with_code("INTERNAL_SERVER_ERROR")
crates/adapteros-server-api-audit/src/handlers.rs:432:                    .with_code("INTERNAL_SERVER_ERROR")
crates/adapteros-server-api-audit/src/handlers.rs:463:                    .with_code("INTERNAL_SERVER_ERROR")
crates/adapteros-server-api-audit/src/handlers.rs:1034:                    .with_code("INVALID_HEX")
crates/adapteros-server-api-audit/src/handlers.rs:1045:                    .with_code("INVALID_HEX")
crates/adapteros-server-api-audit/src/handlers.rs:1056:                    .with_code("INVALID_HEX")
crates/adapteros-server-api-audit/src/handlers.rs:1067:                    .with_code("INVALID_HEX")
crates/adapteros-server-api-audit/src/handlers.rs:1091:                    .with_code("VERIFICATION_ERROR")
crates/adapteros-server-api/src/services/training_dataset.rs:1217:                                .with_code("CONFIG_UNAVAILABLE"),
crates/adapteros-server-api/src/api_error.rs:1042:            Json(ErrorResponse::new("legacy failure").with_code("LEGACY_CODE")),
crates/adapteros-server-api/src/handlers.rs:367:                        .with_code("INTERNAL_SERVER_ERROR")
crates/adapteros-server-api/src/handlers.rs:389:                    .with_code("INTERNAL_SERVER_ERROR")
crates/adapteros-server-api/src/handlers.rs:424:                        .with_code("INTERNAL_SERVER_ERROR")
crates/adapteros-server-api/src/handlers.rs:488:                        .with_code("INTERNAL_SERVER_ERROR")
crates/adapteros-server-api/src/handlers.rs:511:                    .with_code("INTERNAL_SERVER_ERROR")
crates/adapteros-server-api/src/handlers.rs:524:                        .with_code("INTERNAL_SERVER_ERROR")
crates/adapteros-server-api/src/handlers.rs:536:                    .with_code("INTERNAL_SERVER_ERROR")
crates/adapteros-server-api/src/handlers.rs:552:                        .with_code("INTERNAL_SERVER_ERROR")
crates/adapteros-server-api/src/handlers.rs:568:                        .with_code("INTERNAL_SERVER_ERROR")
crates/adapteros-server-api/src/handlers.rs:597:            Json(ErrorResponse::new("failed to insert promotion record").with_code("INTERNAL_SERVER_ERROR").with_string_details(e.to_string())),
crates/adapteros-server-api/src/handlers.rs:607:                    .with_code("INTERNAL_SERVER_ERROR")
crates/adapteros-server-api/src/handlers.rs:1823:                                    .with_code("POLICY_PACK_CORRUPT")
crates/adapteros-server-api/src/handlers.rs:1948:                        .with_code("INTERNAL_SERVER_ERROR")
crates/adapteros-server-api/src/handlers.rs:2085:                    .with_code("INTERNAL_SERVER_ERROR")
crates/adapteros-server-api/src/handlers.rs:2158:                        .with_code("INTERNAL_SERVER_ERROR")
crates/adapteros-server-api/src/handlers.rs:2557:                        .with_code("ADAPTER_FETCH_ERROR")
crates/adapteros-server-api/src/handlers.rs:2596:                        .with_code("ROUTING_ERROR")
crates/adapteros-server-api/src/handlers.rs:2981:                    .with_code("INTERNAL_SERVER_ERROR")
crates/adapteros-server-api/src/handlers.rs:3024:                    .with_code("INTERNAL_SERVER_ERROR")
crates/adapteros-server-api/src/handlers.rs:3127:                        .with_code("INTERNAL_SERVER_ERROR")
crates/adapteros-server-api/src/handlers.rs:3187:                    .with_code("INTERNAL_SERVER_ERROR")
crates/adapteros-server-api/src/handlers.rs:3228:                    .with_code("INTERNAL_SERVER_ERROR")
crates/adapteros-server-api/src/handlers.rs:3290:                    .with_code("INTERNAL_SERVER_ERROR")
crates/adapteros-server-api/src/handlers.rs:3314:                    .with_code("INTERNAL_SERVER_ERROR")
crates/adapteros-server-api/src/handlers.rs:3339:                    .with_code("INTERNAL_SERVER_ERROR")
crates/adapteros-server-api/src/handlers/owner_cli.rs:179:            Json(ErrorResponse::new(e.to_string()).with_code("INVALID_COMMAND")),
crates/adapteros-server-api/src/handlers/owner_cli.rs:247:                        .with_code("EXECUTION_FAILED"),
crates/adapteros-server-api/src/middleware/boot_evidence.rs:55:                    .with_code("BOOT_EVIDENCE_MISSING")
crates/adapteros-server-api/src/middleware/worker_uid.rs:247:        .with_code("WORKER_UID_MISMATCH")
crates/adapteros-server-api/src/middleware/worker_uid.rs:259:        .with_code("PEER_CREDENTIALS_MISSING")
crates/adapteros-server-api/src/middleware/worker_uid.rs:272:        .with_code("PRODUCTION_UCRED_REQUIRED")
crates/adapteros-server-api/src/handlers/services.rs:82:                        .with_code("SUPERVISOR_NOT_CONFIGURED"),
crates/adapteros-server-api/src/handlers/services.rs:151:                        .with_code("SUPERVISOR_NOT_CONFIGURED"),
crates/adapteros-server-api/src/handlers/services.rs:220:                        .with_code("SUPERVISOR_NOT_CONFIGURED"),
crates/adapteros-server-api/src/handlers/services.rs:284:                        .with_code("SUPERVISOR_NOT_CONFIGURED"),
crates/adapteros-server-api/src/handlers/services.rs:344:                        .with_code("SUPERVISOR_NOT_CONFIGURED"),
crates/adapteros-server-api/src/handlers/services.rs:411:                        .with_code("SUPERVISOR_NOT_CONFIGURED"),
crates/adapteros-server-api/src/handlers/tenant_management.rs:51:                            .with_code("INTERNAL_SERVER_ERROR")
crates/adapteros-server-api/src/handlers/tenant_management.rs:68:                            .with_code("INTERNAL_SERVER_ERROR")
crates/adapteros-server-api/src/handlers/tenant_management.rs:81:                    .with_code("INTERNAL_SERVER_ERROR")
crates/adapteros-server-api/src/handlers/tenant_management.rs:131:                    .with_code("INTERNAL_SERVER_ERROR")
crates/adapteros-server-api/src/handlers/tenant_management.rs:171:                    .with_code("INTERNAL_SERVER_ERROR")
crates/adapteros-server-api/src/handlers/tenant_management.rs:650:                            .with_code("INTERNAL_SERVER_ERROR")
crates/adapteros-server-api/src/handlers/worker_detail.rs:120:                        .with_code("INTERNAL_SERVER_ERROR")
crates/adapteros-server-api/src/handlers/worker_detail.rs:128:                Json(ErrorResponse::new("worker not found").with_code("WORKER_NOT_FOUND")),
crates/adapteros-server-api/src/handlers/audit.rs:80:                    .with_code("INTERNAL_SERVER_ERROR")
crates/adapteros-server-api/src/handlers/audit.rs:224:                    .with_code("INTERNAL_SERVER_ERROR")
crates/adapteros-server-api/src/handlers/audit.rs:274:                    .with_code("INTERNAL_SERVER_ERROR")
crates/adapteros-server-api/src/handlers/audit.rs:341:                    .with_code("INTERNAL_SERVER_ERROR")
crates/adapteros-server-api/src/handlers/audit.rs:368:                    .with_code("INTERNAL_SERVER_ERROR")
crates/adapteros-server-api/src/handlers/audit.rs:399:                    .with_code("INTERNAL_SERVER_ERROR")
crates/adapteros-server-api/src/handlers/promotion.rs:1419:                        .with_code("GATES_NOT_READY")
crates/adapteros-server-api/src/handlers/promotion.rs:1444:                    .with_code("CI_NOT_VERIFIED")
crates/adapteros-server-api/src/handlers/node_detail.rs:107:                        .with_code("INTERNAL_SERVER_ERROR")
crates/adapteros-server-api/src/handlers/node_detail.rs:115:                Json(ErrorResponse::new("node not found").with_code("NODE_NOT_FOUND")),
crates/adapteros-server-api/src/handlers/adapter_health.rs:66:                            .with_code("VERIFICATION_FAILED")
crates/adapteros-server-api/src/handlers/discovery.rs:379:                    .with_code("INTERNAL_SERVER_ERROR")
crates/adapteros-server-api/src/handlers/discovery.rs:453:                    .with_code("INTERNAL_SERVER_ERROR")
crates/adapteros-server-api/src/handlers/discovery.rs:498:                        .with_code("INTERNAL_SERVER_ERROR")
crates/adapteros-server-api/src/handlers/discovery.rs:539:                        .with_code("INTERNAL_SERVER_ERROR")
crates/adapteros-server-api/src/handlers/discovery.rs:595:                        .with_code("INTERNAL_SERVER_ERROR")
crates/adapteros-server-api/src/handlers/discovery.rs:617:                    .with_code("INTERNAL_SERVER_ERROR")
crates/adapteros-server-api/src/handlers/routing_decisions.rs:364:                .with_code("ROUTING_CHAIN_ERROR")
crates/adapteros-server-api/src/handlers/routing_decisions.rs:837:                .with_code("ADAPTER_FETCH_ERROR")
crates/adapteros-server-api/src/handlers/routing_decisions.rs:883:                .with_code("ROUTING_ERROR")
crates/adapteros-server-api/src/handlers/infrastructure.rs:40:                    .with_code("INTERNAL_SERVER_ERROR")
crates/adapteros-server-api/src/handlers/infrastructure.rs:86:                        .with_code("INTERNAL_SERVER_ERROR")
crates/adapteros-server-api/src/handlers/infrastructure.rs:97:                    .with_code("INTERNAL_SERVER_ERROR")
crates/adapteros-server-api/src/handlers/infrastructure.rs:184:                        .with_code("INTERNAL_SERVER_ERROR")
crates/adapteros-server-api/src/handlers/infrastructure.rs:268:                        .with_code("INTERNAL_SERVER_ERROR")
crates/adapteros-server-api/src/handlers/infrastructure.rs:319:                    .with_code("INTERNAL_SERVER_ERROR")
crates/adapteros-server-api/src/handlers/infrastructure.rs:344:                    .with_code("INTERNAL_SERVER_ERROR")
crates/adapteros-server-api/src/handlers/infrastructure.rs:416:                    .with_code("INTERNAL_SERVER_ERROR")
crates/adapteros-server-api/src/handlers/infrastructure.rs:556:                    .with_code("AUTH_STATE_ERROR"),
crates/adapteros-server-api/src/handlers/infrastructure.rs:606:                            .with_code("INTERNAL_SERVER_ERROR")
crates/adapteros-server-api/src/handlers/infrastructure.rs:685:                    .with_code("INVALID_ROLE")
crates/adapteros-server-api/src/handlers/infrastructure.rs:749:                    .with_code("INVALID_ROLE")
crates/adapteros-server-api/src/handlers/batch.rs:687:                    .with_code("SESSION_TOKEN_NOT_SUPPORTED")
crates/adapteros-server-api/src/handlers/replay_inference.rs:617:                Json(ErrorResponse::new("Failed to create execution record").with_code("DB_ERROR")),
crates/adapteros-server-api/src/handlers/replay_inference.rs:635:                .with_code("LEGACY_REPLAY_UNSUPPORTED")
crates/adapteros-server-api/src/handlers/replay_inference.rs:892:                Json(ErrorResponse::new("Failed to update execution").with_code("DB_ERROR")),
crates/adapteros-server-api/src/handlers/plugins.rs:46:                Json(ErrorResponse::new(e.to_string()).with_code("PLUGIN_ENABLE_FAILED")),
crates/adapteros-server-api/src/handlers/plugins.rs:88:                Json(ErrorResponse::new(e.to_string()).with_code("PLUGIN_DISABLE_FAILED")),
crates/adapteros-server-api/src/handlers/plugins.rs:127:                Json(ErrorResponse::new(e.to_string()).with_code("PLUGIN_STATUS_FAILED")),
crates/adapteros-server-api/src/handlers/plugins.rs:170:            Json(ErrorResponse::new(e.to_string()).with_code("DB_ERROR")),
crates/adapteros-server-api/src/handlers/plugins.rs:187:                                ErrorResponse::new(e.to_string()).with_code("PLUGIN_CHECK_FAILED"),
crates/adapteros-server-api/src/handlers/adapter_utils.rs:30:                    .with_code("ADAPTER_IN_USE")
crates/adapteros-server-api/src/handlers/auth.rs:69:                        .with_code("USER_NOT_FOUND")
crates/adapteros-server-api/src/handlers/auth_enhanced/stubs.rs:50:                Json(ErrorResponse::new("User not found").with_code("USER_NOT_FOUND")),
crates/adapteros-server-api/src/handlers/auth_enhanced/stubs.rs:79:            Json(ErrorResponse::new("Invalid email format").with_code("INVALID_EMAIL")),
crates/adapteros-server-api/src/handlers/auth_enhanced/stubs.rs:87:            Json(ErrorResponse::new("Display name is required").with_code("INVALID_DISPLAY_NAME")),
crates/adapteros-server-api/src/handlers/auth_enhanced/stubs.rs:99:                .with_code("WEAK_PASSWORD"),
crates/adapteros-server-api/src/handlers/auth_enhanced/stubs.rs:123:                    .with_code("BOOTSTRAP_ALREADY_COMPLETED"),
crates/adapteros-server-api/src/handlers/auth_enhanced/stubs.rs:276:                    .with_code("MFA_NOT_STARTED"),
crates/adapteros-server-api/src/handlers/auth_enhanced/stubs.rs:290:            Json(ErrorResponse::new("Invalid TOTP code").with_code("INVALID_MFA_CODE")),
crates/adapteros-server-api/src/handlers/auth_enhanced/stubs.rs:348:                Json(ErrorResponse::new("MFA secret is missing").with_code("MFA_NOT_STARTED")),
crates/adapteros-server-api/src/handlers/auth_enhanced/stubs.rs:375:                    .with_code("MISSING_MFA_VERIFICATION"),
crates/adapteros-server-api/src/handlers/auth_enhanced/stubs.rs:383:            Json(ErrorResponse::new("Invalid MFA verification code").with_code("INVALID_MFA_CODE")),
crates/adapteros-server-api/src/handlers/auth_enhanced/stubs.rs:416:                .with_code("NOT_IMPLEMENTED"),
crates/adapteros-server-api/src/handlers/auth_enhanced/stubs.rs:437:                .with_code("NOT_IMPLEMENTED"),
crates/adapteros-server-api/src/handlers/debugging.rs:250:                .with_code("MISSING_TABLE");
crates/adapteros-server-api/src/handlers/debugging.rs:326:                .with_code("MISSING_TABLE");
crates/adapteros-server-api/src/handlers/auth_enhanced/tenant_switch.rs:80:            Json(ErrorResponse::new("tenant_id is required").with_code("MISSING_TENANT_ID")),
crates/adapteros-server-api/src/handlers/auth_enhanced/tenant_switch.rs:118:                    .with_code("TENANT_ACCESS_DENIED")
crates/adapteros-server-api/src/handlers/auth_enhanced/tenant_switch.rs:145:                        .with_code("TENANT_NOT_FOUND")
crates/adapteros-server-api/src/handlers/auth_enhanced/tenant_switch.rs:170:                Json(ErrorResponse::new("User not found").with_code("USER_NOT_FOUND")),
crates/adapteros-server-api/src/handlers/auth_enhanced/tenant_switch.rs:177:            Json(ErrorResponse::new("Account is disabled").with_code("ACCOUNT_DISABLED")),
crates/adapteros-server-api/src/handlers/auth_enhanced/tenant_switch.rs:209:                Json(ErrorResponse::new("Token generation failed").with_code("TOKEN_ERROR")),
crates/adapteros-server-api/src/handlers/auth_enhanced/tenant_switch.rs:227:                Json(ErrorResponse::new("Token generation failed").with_code("TOKEN_ERROR")),
crates/adapteros-server-api/src/handlers/auth_enhanced/tenant_switch.rs:296:            Json(ErrorResponse::new("Cookie error").with_code("COOKIE_ERROR")),
crates/adapteros-server-api/src/middleware/mod.rs:169:                .with_code("TENANT_HEADER_MISSING"),
crates/adapteros-server-api/src/handlers/adapter_stacks.rs:150:                    )).with_code("BASE_MODEL_MISMATCH")),
crates/adapteros-server-api/src/handlers/adapter_stacks.rs:880:                                .with_code("ATTACH_MODE_VIOLATION"),
crates/adapteros-server-api/src/handlers/adapter_stacks.rs:900:                                .with_code("ATTACH_MODE_MISMATCH"),
crates/adapteros-server-api/src/handlers/auth_enhanced/refresh.rs:58:            Json(ErrorResponse::new("Missing refresh token").with_code("MISSING_TOKEN")),
crates/adapteros-server-api/src/handlers/auth_enhanced/refresh.rs:85:            Json(ErrorResponse::new("Invalid refresh token").with_code("INVALID_TOKEN")),
crates/adapteros-server-api/src/handlers/auth_enhanced/refresh.rs:132:                        ErrorResponse::new("Invalid refresh token").with_code("ROTATION_MISMATCH"),
crates/adapteros-server-api/src/handlers/auth_enhanced/refresh.rs:147:            Json(ErrorResponse::new("Session expired or invalid").with_code("SESSION_INVALID")),
crates/adapteros-server-api/src/handlers/auth_enhanced/refresh.rs:167:                Json(ErrorResponse::new("User not found").with_code("USER_NOT_FOUND")),
crates/adapteros-server-api/src/handlers/auth_enhanced/refresh.rs:174:            Json(ErrorResponse::new("Account disabled").with_code("ACCOUNT_DISABLED")),
crates/adapteros-server-api/src/handlers/auth_enhanced/refresh.rs:203:                Json(ErrorResponse::new("Token generation failed").with_code("TOKEN_ERROR")),
crates/adapteros-server-api/src/handlers/auth_enhanced/refresh.rs:227:                Json(ErrorResponse::new("Token generation failed").with_code("TOKEN_ERROR")),
crates/adapteros-server-api/src/handlers/adapter_lifecycle.rs:77:                        .with_code("INTERNAL_SERVER_ERROR")
crates/adapteros-server-api/src/handlers/adapter_lifecycle.rs:124:                        .with_code("INTERNAL_SERVER_ERROR")
crates/adapteros-server-api/src/handlers/adapter_lifecycle.rs:194:                        .with_code("INTERNAL_SERVER_ERROR")
crates/adapteros-server-api/src/handlers/adapter_lifecycle.rs:261:                        .with_code("INTERNAL_SERVER_ERROR")
crates/adapteros-server-api/src/handlers/adapter_lifecycle.rs:430:                        .with_code("INTERNAL_SERVER_ERROR")
crates/adapteros-server-api/src/handlers/adapter_lifecycle.rs:477:                        .with_code("INTERNAL_SERVER_ERROR")
crates/adapteros-server-api/src/handlers/adapter_lifecycle.rs:539:                            .with_code("INTERNAL_SERVER_ERROR")
crates/adapteros-server-api/src/handlers/adapter_lifecycle.rs:577:                        .with_code("INTERNAL_SERVER_ERROR")
crates/adapteros-server-api/src/handlers/review.rs:167:                .with_code("PAUSE_NOT_FOUND")
crates/adapteros-server-api/src/handlers/review.rs:173:                .with_code("PAUSE_INFERENCE_MISMATCH")
crates/adapteros-server-api/src/handlers/review.rs:357:            .with_code("PAUSE_NOT_FOUND"),
crates/adapteros-server-api/src/handlers/review.rs:419:            .with_code("PAUSE_NOT_FOUND"),
crates/adapteros-server-api/src/handlers/review.rs:458:                .with_code("PAUSE_NOT_FOUND")
crates/adapteros-server-api/src/handlers/review.rs:505:            .with_code("TRACKER_NOT_AVAILABLE")
crates/adapteros-server-api/src/handlers/auth_enhanced/login.rs:57:                    .with_code("MISSING_CREDENTIALS"),
crates/adapteros-server-api/src/handlers/auth_enhanced/login.rs:87:                        .with_code("ACCOUNT_LOCKED"),
crates/adapteros-server-api/src/handlers/auth_enhanced/login.rs:212:            Json(ErrorResponse::new("Account is disabled").with_code("ACCOUNT_DISABLED")),
crates/adapteros-server-api/src/handlers/auth_enhanced/login.rs:231:                        .with_code("MFA_CONFIG_ERROR"),
crates/adapteros-server-api/src/handlers/auth_enhanced/login.rs:245:                Json(ErrorResponse::new("MFA verification failed").with_code("MFA_SECRET_ERROR")),
crates/adapteros-server-api/src/handlers/auth_enhanced/login.rs:267:                Json(ErrorResponse::new("Invalid MFA code").with_code("INVALID_MFA_CODE")),
crates/adapteros-server-api/src/handlers/auth_enhanced/login.rs:317:                    ErrorResponse::new("Authentication failed").with_code("TOKEN_GENERATION_ERROR"),
crates/adapteros-server-api/src/handlers/auth_enhanced/login.rs:337:                    ErrorResponse::new("Authentication failed").with_code("TOKEN_GENERATION_ERROR"),
crates/adapteros-server-api/src/handlers/auth_enhanced/login.rs:355:            Json(ErrorResponse::new("Authentication failed").with_code("TOKEN_GENERATION_ERROR")),
crates/adapteros-server-api/src/handlers/auth_enhanced/login.rs:386:            Json(ErrorResponse::new("Authentication failed").with_code("SESSION_CREATION_ERROR")),
crates/adapteros-server-api/src/handlers/auth_enhanced/login.rs:457:            Json(ErrorResponse::new("Authentication failed").with_code("COOKIE_ERROR")),
crates/adapteros-server-api/src/handlers/event_applier.rs:529:                    .with_code("TENANT_MISMATCH"),
crates/adapteros-server-api/src/handlers/event_applier.rs:547:                    .with_code("INVALID_EVENT")
crates/adapteros-server-api/src/handlers/event_applier.rs:575:                        .with_code("EVENT_APPLICATION_FAILED")
crates/adapteros-server-api/src/handlers/tutorials.rs:187:            Json(ErrorResponse::new("Invalid tutorial ID").with_code("INVALID_TUTORIAL_ID")),
crates/adapteros-server-api/src/handlers/tutorials.rs:232:            Json(ErrorResponse::new("Invalid tutorial ID").with_code("INVALID_TUTORIAL_ID")),
crates/adapteros-server-api/src/handlers/tutorials.rs:277:            Json(ErrorResponse::new("Invalid tutorial ID").with_code("INVALID_TUTORIAL_ID")),
crates/adapteros-server-api/src/handlers/tutorials.rs:322:            Json(ErrorResponse::new("Invalid tutorial ID").with_code("INVALID_TUTORIAL_ID")),
crates/adapteros-server-api/src/handlers/metrics_time_series.rs:163:                    .with_code("INTERNAL_SERVER_ERROR")
crates/adapteros-server-api/src/handlers/models.rs:600:                    .with_code("HOT_SWAP_GATED")
crates/adapteros-server-api/src/handlers/models.rs:813:                    .with_code("WORKER_UNAVAILABLE")
crates/adapteros-server-api/src/handlers/models.rs:892:                    .with_code("MODEL_COMPATIBILITY_FAILED")
crates/adapteros-server-api/src/handlers/models.rs:1957:                    .with_code("HOT_SWAP_GATED")
crates/adapteros-server-api/src/handlers/admin_lifecycle.rs:247:                .with_code("SUPERVISOR_NOT_CONFIGURED")
crates/adapteros-server-api/src/handlers/admin_lifecycle.rs:312:        .with_code("LIFECYCLE_ERROR")
crates/adapteros-server-api/src/handlers/auth_enhanced/register.rs:76:                    .with_code("REGISTRATION_DISABLED")
crates/adapteros-server-api/src/handlers/auth_enhanced/register.rs:101:                    .with_code("RATE_LIMIT_EXCEEDED")
crates/adapteros-server-api/src/handlers/auth_enhanced/register.rs:114:                    .with_code("INVALID_EMAIL")
crates/adapteros-server-api/src/handlers/auth_enhanced/register.rs:126:                    .with_code("WEAK_PASSWORD")
crates/adapteros-server-api/src/handlers/auth_enhanced/register.rs:142:                        .with_code("EMAIL_EXISTS")
crates/adapteros-server-api/src/handlers/auth_enhanced/register.rs:372:            Json(ErrorResponse::new("session creation failed").with_code("SESSION_ERROR")),
crates/adapteros-server-api/src/handlers/datasets/chunked_handlers.rs:1564:                    .with_code("UPLOAD_ALREADY_COMPLETE"),
crates/adapteros-server-api/src/handlers/datasets/chunked_handlers.rs:1603:                .with_code("INVALID_CHUNK_INDEX"),
crates/adapteros-server-api/src/handlers/datasets/chunked_handlers.rs:1620:                .with_code("INVALID_CHUNK_SIZE"),
crates/adapteros-server-api/src/handlers/datasets/chunked_handlers.rs:1657:                    .with_code("HASH_MISMATCH"),
crates/adapteros-server-api/src/handlers/adapters_read.rs:270:                        .with_code("REPOSITORY_CREATION_FAILED")
crates/adapteros-server-api/src/handlers/adapters_read.rs:330:                        .with_code("REPOSITORY_NOT_FOUND")
crates/adapteros-server-api/src/handlers/adapters_read.rs:460:                        .with_code("POLICY_UPDATE_FAILED")
crates/adapteros-server-api/src/handlers/adapters_read.rs:620:                        .with_code("ARCHIVE_FAILED")
crates/adapteros-server-api/src/handlers/adapters_read.rs:634:                    .with_code("REPOSITORY_NOT_FOUND")
crates/adapteros-server-api/src/handlers/adapters_read.rs:1071:                        .with_code("VERSION_CREATION_FAILED")
crates/adapteros-server-api/src/handlers/adapters_read.rs:1153:                        .with_code("LINEAGE_LOAD_FAILED")
crates/adapteros-server-api/src/handlers/auth_enhanced/dev_bypass.rs:108:                    .with_code("DEV_BYPASS_DISABLED")
crates/adapteros-server-api/src/handlers/auth_enhanced/dev_bypass.rs:515:            Json(ErrorResponse::new("session creation failed").with_code("SESSION_ERROR")),
crates/adapteros-server-api/src/handlers/auth_enhanced/dev_bypass.rs:683:                    .with_code("DEV_BOOTSTRAP_DISABLED")
crates/adapteros-server-api/src/handlers/auth_enhanced/dev_bypass.rs:910:            Json(ErrorResponse::new("session creation failed").with_code("SESSION_ERROR")),
crates/adapteros-server-api/src/handlers/directory_adapters.rs:194:                                    .with_code("INTERNAL_SERVER_ERROR")
crates/adapteros-server-api/src/handlers/directory_adapters.rs:206:                                .with_code("INTERNAL_SERVER_ERROR")
crates/adapteros-server-api/src/handlers/directory_adapters.rs:220:                                .with_code("INTERNAL_SERVER_ERROR")
crates/adapteros-server-api/src/handlers/directory_adapters.rs:237:                            .with_code("INTERNAL_SERVER_ERROR")
crates/adapteros-server-api/src/handlers/directory_adapters.rs:267:                    .with_code("INTERNAL_SERVER_ERROR")
crates/adapteros-server-api/src/handlers/directory_adapters.rs:287:                            .with_code("INTERNAL_SERVER_ERROR")
crates/adapteros-server-api/src/handlers/directory_adapters.rs:305:                                .with_code("INTERNAL_SERVER_ERROR")
crates/adapteros-server-api/src/handlers/directory_adapters.rs:334:                            .with_code("INTERNAL_SERVER_ERROR")
crates/adapteros-server-api/src/handlers/directory_adapters.rs:344:                        .with_code("INTERNAL_SERVER_ERROR")
crates/adapteros-server-api/src/handlers/golden.rs:42:                .with_code("PATH_TRAVERSAL")
crates/adapteros-server-api/src/handlers/golden.rs:53:                    .with_code("PATH_TRAVERSAL")
crates/adapteros-server-api/src/handlers/golden.rs:58:                    .with_code("PATH_TRAVERSAL")
crates/adapteros-server-api/src/handlers/golden.rs:128:                    .with_code("PATH_TRAVERSAL")
crates/adapteros-server-api/src/handlers/golden.rs:191:                    .with_code("PATH_TRAVERSAL")
crates/adapteros-server-api/src/handlers/monitoring/mod.rs:156:                            .with_code("MISSING_TABLE"),
crates/adapteros-server-api/src/handlers/monitoring/mod.rs:355:                            .with_code("MISSING_TABLE"),
crates/adapteros-server-api/src/handlers/monitoring/mod.rs:377:                            .with_code("MISSING_TABLE"),
crates/adapteros-server-api/src/handlers/monitoring/mod.rs:575:                Json(ErrorResponse::new("process_alerts table missing").with_code("MISSING_TABLE")),
crates/adapteros-server-api/src/handlers/monitoring/mod.rs:958:                        .with_code("MISSING_TABLE"),
crates/adapteros-server-api/src/handlers/monitoring/mod.rs:1283:                        .with_code("MISSING_TABLE"),
crates/adapteros-server-api/src/handlers/monitoring/mod.rs:1353:                    .with_code("INTERNAL_SERVER_ERROR")
crates/adapteros-server-api/src/handlers/monitoring/mod.rs:1400:                            .with_code("INTERNAL_SERVER_ERROR")
crates/adapteros-server-api/src/handlers/monitoring/mod.rs:1414:                        .with_code("INTERNAL_SERVER_ERROR")
crates/adapteros-server-api/src/handlers/monitoring/mod.rs:1467:                    .with_code("INTERNAL_SERVER_ERROR")
crates/adapteros-server-api/src/handlers/monitoring/mod.rs:1481:                        .with_code("INTERNAL_SERVER_ERROR")
crates/adapteros-server-api/src/handlers/monitoring/mod.rs:1529:                        .with_code("INTERNAL_SERVER_ERROR")
crates/adapteros-server-api/src/handlers/monitoring/mod.rs:1552:                        .with_code("INTERNAL_SERVER_ERROR")
crates/adapteros-server-api/src/handlers/monitoring/mod.rs:1659:                    .with_code("INTERNAL_SERVER_ERROR")
crates/adapteros-server-api/src/handlers/monitoring/mod.rs:1737:                    .with_code("INTERNAL_SERVER_ERROR")
crates/adapteros-server-api/src/handlers/training.rs:989:                    .with_code("WORKER_CAPABILITY_MISSING"),
crates/adapteros-server-api/src/handlers/training.rs:1011:                .with_code("WORKER_CAPABILITY_MISSING"),
crates/adapteros-server-api/src/handlers/training.rs:1629:                        .with_code("DATASET_ERROR")
crates/adapteros-server-api/src/handlers/training.rs:1644:                        .with_code("DATASET_ERROR")
crates/adapteros-server-api/src/handlers/training.rs:1654:                        .with_code("DATASET_NOT_FOUND")
crates/adapteros-server-api/src/handlers/training.rs:1746:                                .with_code("DATASET_ERROR")
crates/adapteros-server-api/src/handlers/training.rs:1770:                                .with_code("DATASET_VERSION_NOT_FOUND"),
crates/adapteros-server-api/src/handlers/training.rs:1780:                            .with_code("DATASET_ERROR"),
crates/adapteros-server-api/src/handlers/training.rs:1796:                            .with_code("DATASET_ERROR")
crates/adapteros-server-api/src/handlers/training.rs:1827:                    .with_code("INVALID_ADAPTER_TYPE"),
crates/adapteros-server-api/src/handlers/training.rs:1882:                        .with_code("TRAINING_ERROR")
crates/adapteros-server-api/src/handlers/training.rs:1948:                                        .with_code("INVALID_JOB_STATUS")
crates/adapteros-server-api/src/handlers/training.rs:2067:                        .with_code("EXPORT_ERROR"),
crates/adapteros-server-api/src/handlers/training.rs:2238:                    .with_code("PREPROCESSING_DISABLED"),
crates/adapteros-server-api/src/handlers/training.rs:2254:                            .with_code("DATASET_ERROR")
crates/adapteros-server-api/src/handlers/training.rs:2264:                            .with_code("DATASET_NOT_FOUND"),
crates/adapteros-server-api/src/handlers/training.rs:2298:                            .with_code("DATASET_ERROR")
crates/adapteros-server-api/src/handlers/training.rs:2340:                            .with_code("DATASET_ERROR")
crates/adapteros-server-api/src/handlers/training.rs:2355:                            .with_code("DATASET_ERROR")
crates/adapteros-server-api/src/handlers/training.rs:2389:                    .with_code("PREPROCESS_STATUS_ERROR")
crates/adapteros-server-api/src/handlers/training.rs:2885:                    .with_code("INVALID_ADAPTER_TYPE"),
crates/adapteros-server-api/src/handlers/training.rs:3038:                        .with_code("LINEAGE_REQUIRED"),
crates/adapteros-server-api/src/handlers/training.rs:3055:                    .with_code("LINEAGE_REQUIRED"),
crates/adapteros-server-api/src/handlers/training.rs:3068:                .with_code("LINEAGE_REQUIRED"),
crates/adapteros-server-api/src/handlers/training.rs:3121:                .with_code("LINEAGE_REQUIRED"),
crates/adapteros-server-api/src/handlers/training.rs:3202:                            .with_code("DATASET_TRUST_BLOCKED"),
crates/adapteros-server-api/src/handlers/training.rs:3215:                            .with_code("DATASET_TRUST_NEEDS_APPROVAL"),
crates/adapteros-server-api/src/handlers/training.rs:3241:                            .with_code("DATASET_EMPTY")
crates/adapteros-server-api/src/handlers/training.rs:3264:                            .with_code("DATA_SPEC_HASH_MISMATCH"),
crates/adapteros-server-api/src/handlers/training.rs:4204:                            .with_code("INVALID_STATE"),
crates/adapteros-server-api/src/handlers/training.rs:4299:                .with_code("INVALID_STATE"),
crates/adapteros-server-api/src/handlers/training.rs:4309:            ).with_code("NOT_RETRYABLE")),
crates/adapteros-server-api/src/handlers/training.rs:4405:                        .with_code("TRAINING_START_FAILED"),
crates/adapteros-server-api/src/handlers/training.rs:4548:            Json(ErrorResponse::new("Training job has no tenant_id").with_code("NO_TENANT")),
crates/adapteros-server-api/src/handlers/training.rs:4687:            Json(ErrorResponse::new("Training job has no tenant_id").with_code("NO_TENANT")),
crates/adapteros-server-api/src/handlers/training.rs:4698:                    .with_code("JOB_NOT_COMPLETED"),
crates/adapteros-server-api/src/handlers/training.rs:4710:                .with_code("NO_STACK"),
crates/adapteros-server-api/src/handlers/training.rs:5028:                    .with_code("INVALID_PRIORITY"),
crates/adapteros-server-api/src/handlers/training.rs:5350:                Json(ErrorResponse::new("config lock poisoned").with_code("CONFIG_UNAVAILABLE")),
crates/adapteros-server-api/src/handlers/training.rs:5713:            Json(ErrorResponse::new("Too many job IDs (max 100)").with_code("INVALID_REQUEST")),
crates/adapteros-server-api/src/handlers/testkit.rs:93:            .with_code("TESTKIT_PRODUCTION_BLOCKED")
crates/adapteros-server-api/src/handlers/testkit.rs:110:        .with_code("E2E_MODE_DISABLED")
crates/adapteros-server-api/src/handlers/testkit.rs:122:        .with_code("TESTKIT_ERROR")
crates/adapteros-server-api/src/handlers/diag_bundle.rs:203:                    .with_code("MERKLE_ERROR")
crates/adapteros-server-api/src/handlers/diag_bundle.rs:309:                    .with_code("IO_ERROR")
crates/adapteros-server-api/src/handlers/diag_bundle.rs:326:                            .with_code("IO_ERROR")
crates/adapteros-server-api/src/handlers/diag_bundle.rs:343:                        .with_code("IO_ERROR")
crates/adapteros-server-api/src/handlers/diag_bundle.rs:362:                            .with_code("IO_ERROR")
crates/adapteros-server-api/src/handlers/diag_bundle.rs:583:                    .with_code("IO_ERROR")
crates/adapteros-server-api/src/handlers/diag_bundle.rs:613:                    .with_code("PATH_TRAVERSAL")
crates/adapteros-server-api/src/handlers/diag_bundle.rs:625:                    .with_code("IO_ERROR")
crates/adapteros-server-api/src/handlers/diag_bundle.rs:643:                    .with_code("VERIFICATION_ERROR")
crates/adapteros-server-api/src/handlers/diag_bundle.rs:659:                    .with_code("IO_ERROR")
crates/adapteros-server-api/src/handlers/dev_contracts.rs:244:                    .with_code("INVALID_CONTRACT_SAMPLE")
crates/adapteros-server-api/src/handlers/adapter_versions.rs:551:                    .with_code("NOT_SERVEABLE")
crates/adapteros-server-api/src/handlers/datasets/chunked.rs:124:                .with_code("INVALID_CHUNK_INDEX"),
crates/adapteros-server-api/src/handlers/datasets/chunked.rs:173:                .with_code("CHUNK_HASH_MISMATCH"),
crates/adapteros-server-api/src/handlers/datasets/chunked.rs:190:                .with_code("INVALID_CHUNK_SIZE"),
crates/adapteros-server-api/src/handlers/metrics.rs:509:                        .with_code("INTERNAL_SERVER_ERROR")
crates/adapteros-server-api/src/handlers/metrics.rs:947:                        .with_code("INTERNAL_SERVER_ERROR")
crates/adapteros-server-api/src/handlers/datasets/helpers.rs:175:        .with_code("PATH_POLICY_VIOLATION")
crates/adapteros-server-api/src/handlers/git.rs:603:                        .with_code("INTERNAL_SERVER_ERROR"),
crates/adapteros-server-api/src/handlers/git.rs:660:                        .with_code("INTERNAL_SERVER_ERROR"),
crates/adapteros-server-api/src/handlers/adapters/lifecycle.rs:344:                        .with_code("LIFECYCLE_TRANSITION_DENIED")
crates/adapteros-server-api/src/handlers/adapters/lifecycle.rs:471:                    Json(ErrorResponse::new(&msg).with_code("LIFECYCLE_PROMOTION_INVALID")),
crates/adapteros-server-api/src/handlers/adapters/lifecycle.rs:477:                            .with_code("LIFECYCLE_PROMOTION_FAILED")
crates/adapteros-server-api/src/handlers/adapters/lifecycle.rs:585:                    Json(ErrorResponse::new(&msg).with_code("LIFECYCLE_DEMOTION_INVALID")),
crates/adapteros-server-api/src/handlers/adapters/lifecycle.rs:591:                            .with_code("LIFECYCLE_DEMOTION_FAILED")
crates/adapteros-server-api/src/handlers/adapters/import.rs:304:                    .with_code("INVALID_FORMAT"),
crates/adapteros-server-api/src/handlers/adapters/import.rs:317:                        .with_code("INVALID_FORMAT"),
crates/adapteros-server-api/src/handlers/adapters/import.rs:331:                    .with_code("INVALID_FORMAT"),
crates/adapteros-server-api/src/handlers/adapters/import.rs:345:                .with_code("INVALID_FORMAT"),
crates/adapteros-server-api/src/handlers/adapters/import.rs:373:                    .with_code("UNKNOWN_MANIFEST_FIELDS"),
crates/adapteros-server-api/src/handlers/adapters/import.rs:392:                            .with_code("INVALID_FORMAT"),
crates/adapteros-server-api/src/handlers/adapters/import.rs:429:                        .with_code("INVALID_FORMAT"),
crates/adapteros-server-api/src/handlers/adapters/import.rs:682:                            .with_code("NO_TRUSTED_KEY"),
crates/adapteros-server-api/src/handlers/adapters/import.rs:694:                        .with_code("INVALID_SIGNATURE")
crates/adapteros-server-api/src/handlers/adapters/import.rs:706:                        .with_code("INVALID_SIGNATURE"),
crates/adapteros-server-api/src/handlers/adapters/import.rs:719:                        .with_code("INVALID_SIGNATURE")
crates/adapteros-server-api/src/handlers/adapters/import.rs:825:                                .with_code("INVALID_FORMAT"),
crates/adapteros-server-api/src/handlers/adapters/import.rs:841:                                .with_code("INVALID_FORMAT"),
crates/adapteros-server-api/src/handlers/adapters/import.rs:863:                    .with_code("INVALID_FORMAT"),
crates/adapteros-server-api/src/handlers/adapters/import.rs:881:                    .with_code("INVALID_FORMAT"),
crates/adapteros-server-api/src/handlers/adapters/import.rs:925:                    .with_code("MISSING_VERSION"),
crates/adapteros-server-api/src/handlers/adapters/category_policies.rs:206:                        .with_code("INVALID_PARAMETER")
crates/adapteros-server-api/src/handlers/adapters/repo.rs:72:                        .with_code("INVALID_PATH")
crates/adapteros-server-api/src/handlers/adapters/repo.rs:83:                        .with_code("HASH_ERROR")
crates/adapteros-server-api/src/middleware_security.rs:984:                        .with_code("DRAINING")
crates/adapteros-server-api/src/handlers/policies.rs:51:                            .with_code("INTERNAL_SERVER_ERROR")
crates/adapteros-server-api/src/handlers/policies.rs:704:                    .with_code("INVALID_SIGNATURE")
crates/adapteros-server-api/src/handlers/policies.rs:715:                    .with_code("INVALID_SIGNATURE")
crates/adapteros-server-api/src/handlers/policies.rs:739:                    .with_code("INVALID_SIGNATURE")
crates/adapteros-server-api/src/handlers/storage.rs:256:                Json(ErrorResponse::new("config lock poisoned").with_code("CONFIG_UNAVAILABLE")),

## Dynamic Emission Sites (file:line)

crates/adapteros-server-api-models/src/handlers.rs:538:                            .with_code(code)
crates/adapteros-server-api-models/src/handlers.rs:1543:                            .with_code(code)
crates/adapteros-server-api/src/handlers/replay.rs:737:                    .with_code(e.error_code())
crates/adapteros-server-api/src/middleware/observability.rs:98:            .with_code(code.clone())
crates/adapteros-server-api/src/middleware/mod.rs:84:        Json(ErrorResponse::new(message.into()).with_code(code)),
crates/adapteros-server-api/src/api_error.rs:553:        let mut error_response = ErrorResponse::new(&self.message).with_code(normalized.primary);
crates/adapteros-server-api/src/api_error.rs:624:        let mut response = ErrorResponse::new(&err.message).with_code(err.code);
crates/adapteros-server-api/src/types/error.rs:486:        let mut response = ErrorResponse::new(&message).with_code(code);
crates/adapteros-server-api/src/handlers/replay_inference.rs:464:        .with_code(code)
crates/adapteros-server-api/src/handlers/replay_inference.rs:744:        .with_code(code)
crates/adapteros-server-api/src/handlers/replay_inference.rs:817:        .with_code(code)
crates/adapteros-server-api/src/handlers/review.rs:42:        _ => ApiError::internal(e.to_string()).with_code(Cow::Owned(default_code.to_string())),
crates/adapteros-server-api/src/http/mod.rs:353:            .with_code(code)
crates/adapteros-server-api/src/handlers/inference.rs:201:            .with_code(code)
crates/adapteros-server-api/src/handlers/inference.rs:226:            .with_code(code)
crates/adapteros-server-api/src/handlers/inference.rs:497:        .with_code(code)
crates/adapteros-server-api/src/handlers/models.rs:876:                            .with_code(code)
crates/adapteros-server-api/src/handlers/models.rs:1926:                            .with_code(code)
crates/adapteros-server-api/src/handlers/auth_enhanced/stubs.rs:31:        Json(ErrorResponse::new(message.into()).with_code(code)),
crates/adapteros-server-api/src/handlers/adapters/lifecycle.rs:63:                        .with_code(adapteros_core::error_codes::ADAPTER_IN_FLIGHT)
crates/adapteros-server-api/src/handlers/adapters/lifecycle.rs:252:                    .with_code(primary_code)
crates/adapteros-server-api/src/handlers/tenant_policies.rs:236:                            .with_code(AUDIT_CHAIN_DIVERGED_CODE)
crates/adapteros-server-api/src/handlers/training.rs:2648:            Json(ErrorResponse::new(&error_message).with_code(error_code)),
crates/adapteros-server-api/src/handlers/training.rs:2708:                    .with_code(error_code)
crates/adapteros-server-api/src/handlers/training.rs:2816:            Json(ErrorResponse::new(&e.message).with_code(e.code)),
crates/adapteros-server-api/src/handlers/training.rs:5054:                Json(ErrorResponse::new(e.to_string()).with_code(code)),
crates/adapteros-server-api/src/handlers/testkit.rs:2033:                    .with_code(AUDIT_CHAIN_DIVERGED_CODE)
crates/adapteros-server-api/src/handlers/prefix_templates.rs:88:                        .with_code(error_codes::NOT_FOUND),
crates/adapteros-server-api/src/handlers/prefix_templates.rs:117:                        .with_code(error_codes::NOT_FOUND),
crates/adapteros-server-api/src/handlers/prefix_templates.rs:134:                        .with_code(error_codes::NOT_FOUND),
crates/adapteros-server-api/src/handlers/prefix_templates.rs:165:                        .with_code(error_codes::NOT_FOUND),
crates/adapteros-server-api/src/handlers/prefix_templates.rs:188:            Json(ErrorResponse::new("Prefix template not found").with_code(error_codes::NOT_FOUND)),

## Error Enums + Locations

crates/adapteros-agent-spawn/src/error.rs:11:pub enum AgentSpawnError {
crates/adapteros-api-types/src/failure_code.rs:7:pub enum FailureCode {
crates/adapteros-api/src/lib.rs:314:pub enum ApiError {
crates/adapteros-artifacts/src/secureenclave.rs:7:pub enum EnclaveError {
crates/adapteros-auth/src/error.rs:17:pub enum AuthError {
crates/adapteros-base-llm/src/error.rs:10:pub enum BaseLLMError {
crates/adapteros-boot/src/error.rs:67:pub enum WorkerAuthError {
crates/adapteros-boot/src/error.rs:7:pub enum BootError {
crates/adapteros-cli/src/error_codes.rs:43:        pub enum ECode {
crates/adapteros-cli/src/error_codes.rs:870:pub enum ExitCode {
crates/adapteros-client/src/uds.rs:26:pub enum UdsClientError {
crates/adapteros-core/src/adapter_repo_paths.rs:80:pub enum ResolveError {
crates/adapteros-core/src/context_id.rs:37:pub enum ContextIdError {
crates/adapteros-core/src/error.rs:58:pub enum AosError {
crates/adapteros-core/src/error_macros.rs:248:/// pub enum MyLocalError {
crates/adapteros-core/src/error_macros.rs:387:    enum LocalError {
crates/adapteros-core/src/errors/adapter.rs:10:pub enum AosAdapterError {
crates/adapteros-core/src/errors/auth.rs:9:pub enum AosAuthError {
crates/adapteros-core/src/errors/crypto.rs:10:pub enum AosCryptoError {
crates/adapteros-core/src/errors/internal.rs:9:pub enum AosInternalError {
crates/adapteros-core/src/errors/mod.rs:69:pub enum AosError {
crates/adapteros-core/src/errors/model.rs:9:pub enum AosModelError {
crates/adapteros-core/src/errors/network.rs:11:pub enum AosNetworkError {
crates/adapteros-core/src/errors/operations.rs:9:pub enum AosOperationsError {
crates/adapteros-core/src/errors/policy.rs:10:pub enum AosPolicyError {
crates/adapteros-core/src/errors/resource.rs:17:pub enum AosResourceError {
crates/adapteros-core/src/errors/storage.rs:9:pub enum AosStorageError {
crates/adapteros-core/src/errors/validation.rs:9:pub enum AosValidationError {
crates/adapteros-core/src/evidence_verifier.rs:67:pub enum IngestionError {
crates/adapteros-core/src/lifecycle.rs:76:pub enum LifecycleError {
crates/adapteros-core/src/preflight/error.rs:17:pub enum PreflightErrorCode {
crates/adapteros-core/src/receipt_digest.rs:729:pub enum ReceiptDigestError {
crates/adapteros-core/src/recovery/outcome.rs:97:pub enum RecoveryError {
crates/adapteros-core/src/retry_metrics.rs:86:pub enum MetricsError {
crates/adapteros-core/src/singleflight/mod.rs:72://! enum SingleFlightError { LoadFailed(String), Timeout, ... }
crates/adapteros-core/src/validation/error.rs:125:pub enum ValidationErrorCode {
crates/adapteros-crypto/src/receipt_verifier.rs:31:pub enum ReasonCode {
crates/adapteros-db/src/kv_isolation_scan.rs:51:pub enum KvIsolationIssue {
crates/adapteros-db/src/metadata.rs:214:pub enum MetadataValidationError {
crates/adapteros-db/src/retry.rs:463:    enum TestError {
crates/adapteros-deterministic-exec/src/channel.rs:31:pub enum ChannelError {
crates/adapteros-deterministic-exec/src/cpu_affinity.rs:24:pub enum CpuAffinityError {
crates/adapteros-deterministic-exec/src/lib.rs:177:pub enum DeterministicExecutorError {
crates/adapteros-deterministic-exec/src/multi_agent.rs:25:pub enum CoordinationError {
crates/adapteros-deterministic-exec/src/seed.rs:329:pub enum SeedError {
crates/adapteros-domain/src/error.rs:8:pub enum DomainAdapterError {
crates/adapteros-embeddings/src/config.rs:172:pub enum ConfigValidationError {
crates/adapteros-error-registry/src/lib.rs:42:pub enum ECode {
crates/adapteros-federation/src/peer.rs:146:pub enum DiscoveryErrorCode {
crates/adapteros-infra-common/src/error.rs:12:pub enum AosError {
crates/adapteros-lint/src/architectural.rs:18:pub enum ArchitecturalViolation {
crates/adapteros-lora-kernel-mtl/src/coreml.rs:20:pub enum CoreMLErrorCode {
crates/adapteros-lora-kernel-mtl/src/error.rs:6:pub enum KernelError {
crates/adapteros-lora-quant/src/lib.rs:199:pub enum QuantizationError {
crates/adapteros-lora-router/src/types.rs:61:pub enum FeatureVectorError {
crates/adapteros-lora-router/src/types.rs:907:pub enum CodebaseExclusivityError {
crates/adapteros-memory/src/k_reduction_integration.rs:374:pub enum SendError {
crates/adapteros-memory/src/k_reduction_integration.rs:399:pub enum RecvError {
crates/adapteros-memory/src/lib.rs:117:pub enum MemoryWatchdogError {
crates/adapteros-model-hub/src/lib.rs:22:pub enum ModelHubError {
crates/adapteros-numerics/src/noise.rs:169:pub enum NumericsError {
crates/adapteros-policy/src/packs/nvd_client.rs:129:pub enum NvdError {
crates/adapteros-replay/src/lib.rs:46:pub enum ReplayError {
crates/adapteros-replay/src/reproducible.rs:29:pub enum ReproducibleReplayError {
crates/adapteros-replay/src/session.rs:23:pub enum SessionError {
crates/adapteros-replay/src/verification.rs:12:pub enum VerificationError {
crates/adapteros-secd/src/enclave/mod.rs:68:pub enum EnclaveError {
crates/adapteros-secd/src/federation_auth.rs:23:pub enum FederationAuthError {
crates/adapteros-server-api/src/auth_common.rs:164:pub enum AuthError {
crates/adapteros-server-api/src/chat_context.rs:53:pub enum ChatContextError {
crates/adapteros-server-api/src/handlers/adapters/repo.rs:17:pub enum AdapterRepoError {
crates/adapteros-server-api/src/handlers/event_applier.rs:25:pub enum EventApplierError {
crates/adapteros-server-api/src/handlers/replay.rs:933:pub enum ReceiptReasonCode {
crates/adapteros-server-api/src/health.rs:1067:pub enum HealthCheckError {
crates/adapteros-server-api/src/http/mod.rs:316:enum ApiError {
crates/adapteros-server-api/src/lifecycle.rs:219:pub enum ShutdownError {
crates/adapteros-server-api/src/model_runtime.rs:84:pub enum ModelLoadError {
crates/adapteros-server-api/src/operation_tracker.rs:1043:pub enum OperationCancellationError {
crates/adapteros-server-api/src/rate_limit.rs:88:pub enum RateLimitError {
crates/adapteros-server-api/src/retry.rs:213:pub enum CircuitBreakerError<E: std::fmt::Display> {
crates/adapteros-server-api/src/types/error.rs:88:pub enum InferenceError {
crates/adapteros-server-api/src/uds_client.rs:108:pub enum UdsClientError {
crates/adapteros-server-api/src/worker_selector.rs:495:pub enum SelectionError {
crates/adapteros-server/src/boot/server.rs:106:pub enum BindError {
crates/adapteros-service-supervisor/src/error.rs:10:pub enum SupervisorError {
crates/adapteros-storage/src/adapter_refs.rs:256:pub enum AdapterNameError {
crates/adapteros-storage/src/backend.rs:20:pub enum StorageError {
crates/adapteros-storage/src/error.rs:7:pub enum StorageError {
crates/adapteros-storage/src/search.rs:21:pub enum SearchError {
crates/adapteros-telemetry/src/alerting.rs:75:pub enum AlertDispatchError {
crates/adapteros-telemetry/src/diagnostics/mod.rs:515:pub enum DiagError {
crates/adapteros-telemetry/src/diagnostics/writer.rs:69:pub enum PersistError {
crates/adapteros-telemetry/src/events/telemetry_events.rs:776:pub enum BudgetViolation {
crates/adapteros-types/src/inference.rs:26:pub enum StopReasonCode {
crates/adapteros-types/src/training/example.rs:503:pub enum PreferencePairValidationError {
crates/adapteros-types/src/training/example.rs:652:pub enum TrainingExampleValidationError {
crates/adapteros-ui/src/api/error.rs:13:pub enum ApiError {
crates/adapteros-ui/src/api/mod.rs:84:pub enum ApiBaseUrlError {
crates/adapteros-ui/src/signals/auth.rs:66:pub enum AuthError {
crates/adapteros-verify/src/lib.rs:70:pub enum VerifyError {
crates/adapteros-web-browse/src/error.rs:11:pub enum WebBrowseError {
crates/adapteros-web-browse/src/rate_limit.rs:113:enum RateLimitError {
