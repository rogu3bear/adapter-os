# UI Routes

High-level route map for the adapterOS Leptos UI. Use this to trace which components mount, which API calls are expected, and what permissions are needed.

## Routes

| Path | Component | Key API calls | Permissions |
| --- | --- | --- | --- |
| `/login` | `pages/login.rs` (`Login`) | `POST /v1/auth/login` | Public |
| `/` and `/dashboard` | `pages/dashboard.rs` (`Dashboard`) inside `Shell` | Various stats (service status, metrics) | Authenticated (ProtectedRoute) |
| `/datasets` | `pages/datasets.rs` (`Datasets`) | `GET /v1/datasets` | Authenticated |
| `/datasets/:id` | `pages/documents.rs` (`DatasetDetail` inside documents module) | `GET /v1/datasets/{id}`, dataset files | Authenticated |
| `/training` | `pages/training/mod.rs` (`Training`) | `GET /v1/training/jobs`, `GET /v1/training/backend-readiness`, dialogs use `POST /v1/training/start` | Authenticated |
| `/chat` | `pages/chat/mod.rs` (`Chat`) | `GET /v1/chat_sessions`, `POST /v1/chat_sessions`, `POST /v1/infer` (stream) | Authenticated |
| `/chat/:session_id` | `pages/chat/mod.rs` (`ChatSession`) | `GET /v1/chat_sessions/{id}`, chat SSE/infer | Authenticated |
| `/adapters` | `pages/adapters/mod.rs` (`Adapters`) | `GET /v1/adapters` | Authenticated |
| `/adapters/:id` | `pages/adapters/detail.rs` (`AdapterDetail`) | `GET /v1/adapters/{id}`, manifests | Authenticated |
| `/stacks` | `pages/stacks/mod.rs` (`Stacks`) | `GET /v1/stacks` | Authenticated |
| `/stacks/:id` | `pages/stacks/detail.rs` (`StackDetail`) | `GET /v1/stacks/{id}` | Authenticated |
| `/models` | `pages/models.rs` (`Models`) | `GET /v1/models` | Authenticated |
| `/workers` | `pages/workers/mod.rs` (`Workers`) | `GET /v1/workers` | Authenticated |
| `/workers/:id` | `pages/workers/detail.rs` (`WorkerDetail`) | `GET /v1/workers/{id}` | Authenticated |
| `/collections` | `pages/collections.rs` (`Collections`) | `GET /v1/collections` | Authenticated |
| `/collections/:id` | `pages/collections.rs` (`CollectionDetail`) | `GET /v1/collections/{id}` | Authenticated |
| `/documents` | `pages/documents/mod.rs` (`Documents`) | `GET /v1/documents` | Authenticated |
| `/documents/:id` | `pages/documents/detail.rs` (`DocumentDetail`) | `GET /v1/documents/{id}` | Authenticated |
| `/routing` | `pages/routing.rs` (`Routing`) | Router debug APIs | Authenticated |
| `/repositories` | `pages/repositories/mod.rs` (`Repositories`) | `GET /v1/repos` | Authenticated |
| `/repositories/:id` | `pages/repositories/detail.rs` (`RepositoryDetail`) | `GET /v1/repos/{id}` | Authenticated |
| `/audit` | `pages/audit.rs` (`Audit`) | Audit feed APIs | Authenticated (admin/operator) |
| `/admin` | `pages/admin.rs` (`Admin`) | Admin APIs (lifecycle/settings) | Admin/operator |
| `/monitoring` | `pages/monitoring.rs` (`Monitoring`) | Metrics APIs | Authenticated |
| `/errors` | `pages/errors.rs` (`Errors`) | Error feed APIs | Authenticated |
| `/runs` | `pages/flight_recorder/mod.rs` (`FlightRecorder`) | `GET /v1/runs` | Authenticated |
| `/runs/:id` | `pages/flight_recorder/detail.rs` (`FlightRecorderDetail`) | `GET /v1/runs/{id}` | Authenticated |
| `/policies` | `pages/policies.rs` (`Policies`) | Policy APIs | Authenticated |
| `/system` | `pages/system.rs` (`System`) | System status APIs | Authenticated |
| `/settings` | `pages/settings.rs` (`Settings`) | Settings APIs | Authenticated |
| `/safe` | `pages/safe.rs` (`Safe`) | None | Public |
| `/style-audit` | `pages/style_audit.rs` (`StyleAudit`) | None | Public |

## Golden Flows (human click script)

### Upload dataset (UI)
1. Navigate to `/datasets`.
2. Click “Upload” (or equivalent CTA) → choose JSONL file with prompt/response fields (matches `ValidationConfig::for_training_jsonl`).
3. Submit; wait for status `ready` in list.
4. Open dataset detail `/datasets/:id` to confirm rows/files are listed.

### Start training
1. Go to `/training`.
2. Click “New Training Job”.
3. Pick adapter name, select dataset version, choose base model, keep quick config (low rank/epochs).
4. Submit; job appears in list; poll until status `running` → `completed`.
5. Optional: open job detail panel for metrics/report once completed.

### Test inference / chat
1. Go to `/chat`.
2. Start new chat session.
3. In request settings, pin the newly trained adapter/stack (if supported by UI).
4. Send a short prompt; expect streamed tokens and a non-empty completion.

Permissions: all flows use ProtectedRoute → requires authenticated user (admin/operator/user depending on backend auth config). Upload/train typically require dataset upload and training permissions (admin/operator/user with proper role).
