# 100 Backend Questions Necessary to Finish the Frontend

**Purpose:** Comprehensive list of backend API questions that must be answered to complete frontend implementation  
**Date:** 2025-01-15  
**Status:** Questions requiring backend team answers

---

## Table of Contents

1. [Authentication & Authorization (10 questions)](#1-authentication--authorization)
2. [API Endpoints & Data Structures (15 questions)](#2-api-endpoints--data-structures)
3. [Real-time/SSE Streaming (8 questions)](#3-real-timesse-streaming)
4. [Error Handling & Validation (7 questions)](#4-error-handling--validation)
5. [Training Jobs (10 questions)](#5-training-jobs)
6. [Adapters Management (10 questions)](#6-adapters-management)
7. [Metrics & Telemetry (8 questions)](#7-metrics--telemetry)
8. [Nodes & Workers (7 questions)](#8-nodes--workers)
9. [Policies & Control Plane (6 questions)](#9-policies--control-plane)
10. [Tenants (5 questions)](#10-tenants)
11. [Models (6 questions)](#11-models)
12. [Routing & Inference (5 questions)](#12-routing--inference)
13. [Workspaces & Collaboration (4 questions)](#13-workspaces--collaboration)
14. [Logs & Monitoring (3 questions)](#14-logs--monitoring)
15. [File Operations (2 questions)](#15-file-operations)

---

## 1. Authentication & Authorization

### Q1: What is the exact JWT token structure and all available claims?
**Context:** Frontend needs to parse and display user info, tenant context, roles  
**Current State:** `Claims` struct exists in backend but frontend needs complete schema  
**Required:** Complete JWT payload schema with all fields, types, and optional vs required

### Q2: How does token refresh work and what happens on expiration?
**Context:** Frontend needs to handle token refresh gracefully  
**Current State:** `/v1/auth/refresh` endpoint exists  
**Required:** 
- Refresh token flow (is there a separate refresh token?)
- Automatic refresh timing (before expiration?)
- What happens if refresh fails?
- Does refresh update the httpOnly cookie?

### Q3: What are all the available user roles and their permission levels?
**Context:** Frontend needs to show/hide UI elements based on roles  
**Current State:** Roles mentioned: Admin, Operator, Compliance, Viewer  
**Required:** Complete role hierarchy, permissions per role, role display names

### Q4: How does session management work across multiple tabs/devices?
**Context:** Frontend needs to handle concurrent sessions  
**Current State:** `/v1/auth/sessions` endpoint exists  
**Required:**
- Can users have multiple active sessions?
- How are sessions tracked (device, IP, user agent)?
- What happens when user logs out from one device?

### Q5: What is the dev-bypass token behavior in production?
**Context:** Frontend may need to handle different auth modes  
**Current State:** `/v1/auth/dev-bypass` exists  
**Required:**
- Is dev-bypass available in production?
- What are the security implications?
- Should frontend show different UI for dev vs production auth?

### Q6: How does cookie-based authentication work with CORS?
**Context:** Frontend needs to ensure cookies are sent with requests  
**Current State:** API client uses `credentials: 'include'`  
**Required:**
- Cookie name and httpOnly flag?
- SameSite policy?
- CORS configuration for cookie auth?
- Does cookie work with SSE connections?

### Q7: What happens when a user's role changes while they're logged in?
**Context:** Frontend needs to handle permission changes  
**Required:**
- Do permissions update immediately or require re-login?
- Should frontend poll for role changes?
- What happens to active sessions when role is downgraded?

### Q8: How does tenant isolation work in multi-tenant scenarios?
**Context:** Frontend needs to filter data by tenant  
**Current State:** JWT contains `tenant_id` claim  
**Required:**
- How is tenant_id enforced on backend?
- Can users belong to multiple tenants?
- What happens if tenant_id is missing from JWT?

### Q9: What is the exact error response format for authentication failures?
**Context:** Frontend needs to display user-friendly auth errors  
**Current State:** `ErrorResponse` type exists  
**Required:**
- All possible auth error codes
- Error message format
- When to show login form vs error message

### Q10: How does API token rotation work and what triggers it?
**Context:** Frontend may need to handle token rotation  
**Current State:** `/v1/auth/token/rotate` endpoint exists  
**Required:**
- When should tokens be rotated?
- Does rotation invalidate old tokens?
- How does frontend handle rotation during active session?

---

## 2. API Endpoints & Data Structures

### Q11: What is the exact response structure for `/v1/status`?
**Context:** Dashboard displays system status  
**Current State:** `AdapterOSStatus` type exists in frontend  
**Required:** Complete schema with all fields, optional vs required, example responses

### Q12: What are all the query parameters supported by `/v1/adapters`?
**Context:** Adapter list page needs filtering  
**Current State:** `tier` and `framework` params mentioned  
**Required:**
- Complete list of query params
- Valid values for each param
- Default values
- Pagination support?

### Q13: What is the exact structure of adapter state transitions?
**Context:** Frontend needs to show adapter lifecycle states  
**Current State:** States: unloaded, cold, warm, hot, resident  
**Required:**
- All possible state transitions
- What triggers each transition?
- Can adapters skip states?
- How long do transitions take?

### Q14: What is the complete schema for training job responses?
**Context:** Training page displays job details  
**Current State:** `TrainingJob` type exists but incomplete  
**Required:**
- All fields in response
- Progress calculation (is it percentage or fraction?)
- Error message structure
- Metrics structure

### Q15: What are all the possible training job statuses?
**Context:** Frontend needs to show correct status indicators  
**Current State:** Statuses: pending, running, completed, failed, cancelled  
**Required:**
- Complete list of statuses
- Status transition rules
- Can jobs be paused/resumed?
- What does "cancelled" mean vs "failed"?

### Q16: What is the exact structure of inference request/response?
**Context:** Inference testing UI needs correct data format  
**Current State:** `InferRequest` and `InferResponse` types exist  
**Required:**
- All optional vs required fields
- Valid ranges for temperature, top_k, top_p
- Trace structure details
- Streaming response format

### Q17: What is the structure of routing decisions response?
**Context:** Routing debug page needs to display decisions  
**Current State:** `RoutingDecision` type exists  
**Required:**
- Complete field list
- How are adapters selected?
- What do gate values represent?
- Confidence scores meaning

### Q18: What are all the metrics endpoints and their response structures?
**Context:** Metrics dashboard needs correct data  
**Current State:** Multiple metrics endpoints exist  
**Required:**
- `/v1/metrics/system` complete schema
- `/v1/metrics/quality` complete schema
- `/v1/metrics/adapters` complete schema
- Units for all metric values
- Update frequency

### Q19: What is the exact structure of node details response?
**Context:** Nodes page displays node information  
**Current State:** `NodeDetailsResponse` type exists  
**Required:**
- All fields in response
- Worker information structure
- Recent logs format
- Resource usage fields

### Q20: What are all the query parameters for telemetry events?
**Context:** Activity feed needs filtering  
**Current State:** `limit`, `tenant_id`, `event_type` mentioned  
**Required:**
- Complete list of query params
- Valid event types
- Date range filtering?
- Sorting options?

### Q21: What is the exact structure of policy pack responses?
**Context:** Policy management UI needs correct data  
**Current State:** `Policy` and `PolicyPackResponse` types exist  
**Required:**
- Policy JSON schema
- Validation error format
- Policy comparison result structure

### Q22: What is the structure of tenant usage response?
**Context:** Tenant management page displays usage  
**Current State:** `TenantUsageResponse` type exists  
**Required:**
- All fields in response
- Time period for usage metrics
- Cost calculation if applicable

### Q23: What are all the possible adapter categories and their meanings?
**Context:** Adapter filtering and display  
**Current State:** Categories: code, framework, codebase, ephemeral  
**Required:**
- Complete list of categories
- Category-specific fields
- Category policies structure

### Q24: What is the exact structure of repository analysis response?
**Context:** Repository management page  
**Current State:** `RepositoryAnalysis` type exists  
**Required:**
- Complete analysis structure
- Evidence spans format
- Security scan results format

### Q25: What are all the query parameters for workspace endpoints?
**Context:** Workspace management UI  
**Current State:** Workspace endpoints exist  
**Required:**
- Filtering options
- Pagination support
- Member role types
- Resource sharing structure

---

## 3. Real-time/SSE Streaming

### Q26: What is the exact SSE event format for metrics stream?
**Context:** Real-time metrics dashboard  
**Current State:** `/v1/stream/metrics` endpoint exists  
**Required:**
- Event type names
- Data payload structure
- Keep-alive interval
- Error event format

### Q27: How does the training job progress SSE stream work?
**Context:** Training page needs real-time updates  
**Current State:** Training stream mentioned but not fully documented  
**Required:**
- Endpoint URL
- Event types
- Progress update frequency
- How to handle reconnection?

### Q28: What is the SSE format for adapter state changes?
**Context:** Adapter list needs real-time state updates  
**Current State:** `/v1/stream/adapters` endpoint exists  
**Required:**
- Event types for state transitions
- Payload structure
- How to handle missed events?

### Q29: How does the activity feed SSE stream work?
**Context:** Dashboard recent activity needs real-time updates  
**Current State:** `/v1/telemetry/events/recent/stream` exists  
**Required:**
- Initial backlog size
- Event format
- Filtering support in stream
- Deduplication logic

### Q30: What happens when SSE connection is lost?
**Context:** Frontend needs to handle disconnections gracefully  
**Required:**
- Reconnection strategy
- How to resume from last event?
- What happens to missed events?
- Should frontend fall back to polling?

### Q31: How does authentication work with SSE streams?
**Context:** SSE connections need auth  
**Current State:** EventSource doesn't support custom headers  
**Required:**
- Can cookies be used with SSE?
- Query parameter auth support?
- Token expiration handling

### Q32: What is the SSE format for notifications stream?
**Context:** Notification system needs real-time updates  
**Current State:** `/v1/stream/notifications` exists  
**Required:**
- Event types
- Notification payload structure
- Unread count updates

### Q33: How does the alerts SSE stream work?
**Context:** Alerts page needs real-time updates  
**Current State:** `/v1/monitoring/alerts/stream` exists  
**Required:**
- Alert event format
- Severity levels
- Acknowledgment events

---

## 4. Error Handling & Validation

### Q34: What are all possible error codes and their meanings?
**Context:** Frontend needs to display appropriate error messages  
**Current State:** `ErrorResponse` type exists  
**Required:**
- Complete list of error codes
- HTTP status code mapping
- User-friendly messages for each code

### Q35: What is the validation error format for invalid requests?
**Context:** Form validation needs to show field-level errors  
**Required:**
- Field-level error structure
- Validation rule messages
- How to map backend errors to form fields

### Q36: How are rate limit errors communicated?
**Context:** Frontend needs to handle rate limiting  
**Current State:** Rate limiting middleware exists  
**Required:**
- Rate limit error format
- Retry-after header?
- Rate limit headers?

### Q37: What happens when a resource is not found?
**Context:** 404 handling  
**Required:**
- Error response format
- When to show 404 page vs error message
- Resource type in error response

### Q38: How are permission errors (403) handled?
**Context:** Frontend needs to show appropriate messages  
**Required:**
- Error response format
- Should frontend hide UI elements proactively?
- How to handle permission changes?

### Q39: What is the timeout behavior for long-running operations?
**Context:** Training jobs, model loading, etc.  
**Required:**
- Request timeout values
- How to handle timeouts?
- Should frontend use longer timeouts for certain operations?

### Q40: How are validation errors for file uploads handled?
**Context:** Adapter import, model import  
**Required:**
- File size limit errors
- File type validation errors
- Upload progress errors

---

## 5. Training Jobs

### Q41: What is the exact request structure for starting training?
**Context:** Training wizard needs correct request format  
**Current State:** `StartTrainingRequest` type exists  
**Required:**
- All required vs optional fields
- Valid value ranges
- Directory vs dataset path usage
- Packaging options

### Q42: How is training progress calculated and reported?
**Context:** Progress bar needs accurate progress  
**Required:**
- Progress calculation method
- Update frequency
- Epoch vs overall progress
- Can progress go backwards?

### Q43: What happens when training is cancelled?
**Context:** Cancel button behavior  
**Required:**
- Can training be cancelled at any time?
- What is the final status after cancel?
- Are partial artifacts available?

### Q44: What is the structure of training artifacts response?
**Context:** Download trained adapter  
**Current State:** `TrainingArtifactsResponse` type exists  
**Required:**
- Artifact path format
- Download URL structure
- Signature verification status
- When are artifacts available?

### Q45: How are training logs retrieved and formatted?
**Context:** Training page shows logs  
**Required:**
- Log endpoint structure
- Log format (plain text? JSON?)
- Log size limits
- Real-time log streaming?

### Q46: What are training templates and how are they used?
**Context:** Template selection in training wizard  
**Required:**
- Template structure
- How to create templates?
- Template parameters
- Default templates

### Q47: What is the training session vs job distinction?
**Context:** Two different training concepts exist  
**Required:**
- When to use sessions vs jobs?
- Can sessions be paused/resumed?
- Session lifecycle

### Q48: How are training metrics structured?
**Context:** Training metrics display  
**Required:**
- Metrics response structure
- Available metrics
- Update frequency
- Historical metrics?

### Q49: What happens if training fails?
**Context:** Error handling  
**Required:**
- Error message format
- Partial results available?
- Retry mechanism?
- Failure reasons

### Q50: How does training work with repositories?
**Context:** Repository-based training  
**Required:**
- Repository selection process
- Evidence extraction process
- Training data preparation
- Repository scanning status

---

## 6. Adapters Management

### Q51: What is the exact structure of adapter import request?
**Context:** Adapter import UI  
**Required:**
- File upload format (multipart?)
- File size limits
- Supported file types
- Load option behavior

### Q52: How does adapter pinning work with TTL?
**Context:** Memory management UI  
**Required:**
- Pin request structure
- TTL format (hours? seconds?)
- What happens when TTL expires?
- Can TTL be updated?

### Q53: What is the adapter state promotion process?
**Context:** Adapter lifecycle management  
**Required:**
- Promotion request structure
- State transition rules
- Promotion triggers
- Can promotion be forced?

### Q54: How are adapter activations tracked and reported?
**Context:** Adapter performance metrics  
**Required:**
- Activation structure
- Activation percentage meaning
- Time window for activations
- Historical activation data?

### Q55: What is the adapter health check response structure?
**Context:** Adapter health display  
**Required:**
- Health check fields
- Health status values
- Policy violations format
- Recent activations structure

### Q56: How does adapter swapping work?
**Context:** Hot-swap functionality  
**Required:**
- Swap request structure
- Commit behavior
- Rollback support?
- Swap validation

### Q57: What is the adapter manifest structure?
**Context:** Adapter details display  
**Required:**
- Complete manifest schema
- Download format
- Manifest validation
- Version information

### Q58: How are adapter category policies managed?
**Context:** Category policy configuration  
**Required:**
- Policy structure per category
- Policy update process
- Policy inheritance?
- Default policies

### Q59: What happens when adapter is deleted?
**Context:** Delete confirmation and cleanup  
**Required:**
- Delete request format
- Cascade deletion?
- Orphaned references?
- Recovery possible?

### Q60: How does bulk adapter loading work?
**Context:** Bulk operations UI  
**Required:**
- Bulk load request structure
- Operation status tracking
- Partial success handling
- Progress reporting

---

## 7. Metrics & Telemetry

### Q61: What is the exact structure of system metrics response?
**Context:** Metrics dashboard  
**Required:**
- All metric fields
- Units for each metric
- Update frequency
- Historical data available?

### Q62: How are quality metrics calculated?
**Context:** Quality metrics display  
**Required:**
- Metrics included (ARR, ECS5, HLR, CR)
- Calculation method
- Time window
- Historical trends?

### Q63: What is the adapter metrics response structure?
**Context:** Per-adapter metrics  
**Required:**
- Metrics per adapter
- Aggregation method
- Time range
- Performance metrics details

### Q64: How are telemetry bundles generated and exported?
**Context:** Telemetry export functionality  
**Required:**
- Bundle generation process
- Export format
- Download URL structure
- Bundle expiration

### Q65: What is the telemetry event structure?
**Context:** Activity feed  
**Required:**
- Complete event schema
- Event types
- Metadata structure
- Tenant filtering

### Q66: How does telemetry bundle signature verification work?
**Context:** Compliance features  
**Required:**
- Verification request/response
- Signature format
- Verification errors
- Trust chain

### Q67: What is the structure of recent activity events?
**Context:** Dashboard recent activity  
**Required:**
- Event format
- Event type values
- Metadata fields
- Sorting and filtering

### Q68: How are telemetry logs queried?
**Context:** Log viewer  
**Required:**
- Query parameters
- Response format
- Log levels
- Time range filtering

---

## 8. Nodes & Workers

### Q69: What is the node registration process?
**Context:** Node management UI  
**Required:**
- Registration request structure
- Required vs optional fields
- Validation rules
- Registration confirmation

### Q70: How does node health checking work?
**Context:** Node status display  
**Required:**
- Health check endpoint
- Health status values
- Check frequency
- Health check failures

### Q71: What is the worker spawn request structure?
**Context:** Worker management  
**Required:**
- Spawn request fields
- Resource requirements
- Worker configuration
- Spawn status tracking

### Q72: How are worker logs retrieved?
**Context:** Worker debugging  
**Required:**
- Log endpoint structure
- Log format
- Log filtering
- Real-time log streaming?

### Q73: What happens when a worker crashes?
**Context:** Error handling  
**Required:**
- Crash detection
- Crash report structure
- Recovery process
- Crash history

### Q74: How does node cordoning and draining work?
**Context:** Node maintenance  
**Required:**
- Cordon request format
- Drain process
- Worker migration
- Status updates

### Q75: What is the node details response structure?
**Context:** Node information display  
**Required:**
- Complete node details schema
- Worker list structure
- Resource usage fields
- Recent activity

---

## 9. Policies & Control Plane

### Q76: What is the exact policy JSON schema?
**Context:** Policy editor  
**Required:**
- Complete policy schema
- Validation rules
- Policy versioning
- Schema evolution

### Q77: How does policy validation work?
**Context:** Policy validation UI  
**Required:**
- Validation request format
- Validation error structure
- Validation rules
- Schema validation

### Q78: What is the policy comparison result structure?
**Context:** Policy diff view  
**Required:**
- Comparison request format
- Diff structure
- Change types
- Conflict resolution

### Q79: How does control plane promotion work?
**Context:** Promotion UI  
**Required:**
- Promotion request structure
- Gate checking process
- Promotion status
- Rollback process

### Q80: What are promotion gates and how are they checked?
**Context:** Promotion gate display  
**Required:**
- Gate structure
- Gate checking logic
- Gate results format
- Gate bypass options

### Q81: How does policy signing work?
**Context:** Policy signing UI  
**Required:**
- Signing request format
- Signature structure
- Signing authority
- Signature verification

---

## 10. Tenants

### Q82: What is the tenant creation request structure?
**Context:** Tenant management UI  
**Required:**
- Required vs optional fields
- Isolation level options
- Validation rules
- Default settings

### Q83: How does tenant pausing work?
**Context:** Tenant lifecycle management  
**Required:**
- Pause request format
- What happens to active operations?
- Resume process
- Pause duration limits?

### Q84: How are tenant policies assigned?
**Context:** Policy assignment UI  
**Required:**
- Assignment request structure
- Policy validation
- Assignment conflicts
- Policy priority

### Q85: What is the tenant usage calculation method?
**Context:** Usage display  
**Required:**
- Usage metrics included
- Time period
- Aggregation method
- Cost calculation?

### Q86: How does tenant archiving work?
**Context:** Tenant lifecycle  
**Required:**
- Archive request format
- Data retention
- Archive recovery
- Archive status

---

## 11. Models

### Q87: What is the model import request structure?
**Context:** Model import UI  
**Required:**
- Import request fields
- File paths required
- Import status tracking
- Import validation

### Q88: How does model loading work?
**Context:** Model management  
**Required:**
- Load request format
- Loading status
- Memory requirements
- Load time estimation

### Q89: What is the model status response structure?
**Context:** Model status display  
**Required:**
- Status values
- Status fields
- Memory usage
- Error information

### Q90: How does model validation work?
**Context:** Model validation UI  
**Required:**
- Validation request format
- Validation checks
- Validation results
- Download commands format

### Q91: What is the model download response structure?
**Context:** Model download UI  
**Required:**
- Download URL structure
- Artifact format
- Expiration handling
- Download progress?

### Q92: How does Cursor configuration work?
**Context:** Cursor integration  
**Required:**
- Config response structure
- Setup instructions format
- Configuration validation
- Model readiness check

---

## 12. Routing & Inference

### Q93: What is the routing debug request/response structure?
**Context:** Routing debug UI  
**Required:**
- Debug request format
- Response structure
- Feature vector details
- Adapter scores meaning

### Q94: How are routing decisions stored and retrieved?
**Context:** Routing history display  
**Required:**
- Decision query parameters
- Decision structure
- Historical data retention
- Decision filtering

### Q95: What is the inference streaming response format?
**Context:** Streaming inference UI  
**Required:**
- Stream format (SSE? WebSocket?)
- Chunk structure
- Completion detection
- Error handling in stream

### Q96: How does batch inference work?
**Context:** Batch inference UI  
**Required:**
- Batch request structure
- Batch response format
- Partial failures handling
- Progress tracking

### Q97: What is the inference trace structure?
**Context:** Inference debugging  
**Required:**
- Trace structure details
- Router decisions format
- Evidence spans format
- Performance metrics

---

## 13. Workspaces & Collaboration

### Q98: What is the workspace member role structure?
**Context:** Workspace management  
**Required:**
- Available roles
- Role permissions
- Role assignment
- Role hierarchy

### Q99: How does workspace resource sharing work?
**Context:** Resource sharing UI  
**Required:**
- Sharing request format
- Resource types
- Sharing permissions
- Unsharing process

### Q100: What is the message and notification structure?
**Context:** Collaboration features  
**Required:**
- Message format
- Notification types
- Thread structure
- Notification delivery

---

## Summary

These 100 questions cover all major areas where frontend needs backend API details:

- **Authentication & Authorization (10):** Complete auth flow, tokens, sessions, roles
- **API Endpoints & Data Structures (15):** Request/response schemas, query params, status codes
- **Real-time/SSE Streaming (8):** SSE formats, reconnection, event types
- **Error Handling & Validation (7):** Error codes, validation formats, error messages
- **Training Jobs (10):** Training lifecycle, progress, artifacts, logs
- **Adapters Management (10):** Adapter operations, state management, health checks
- **Metrics & Telemetry (8):** Metrics structures, telemetry events, bundles
- **Nodes & Workers (7):** Node operations, worker management, health checks
- **Policies & Control Plane (6):** Policy management, promotion, validation
- **Tenants (5):** Tenant lifecycle, usage, policies
- **Models (6):** Model operations, status, validation
- **Routing & Inference (5):** Routing decisions, inference formats, traces
- **Workspaces & Collaboration (3):** Workspace features, messaging

**Next Steps:**
1. Backend team should answer these questions with complete API documentation
2. Frontend team should update TypeScript types based on answers
3. Create integration tests for each API endpoint
4. Update OpenAPI specification with complete schemas

