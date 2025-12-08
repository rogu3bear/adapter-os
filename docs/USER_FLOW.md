# User Flow: Login to Trained Adapter and Model Inference

**Complete Implementation Guide** - Every detail required to implement the full user flow from login to trained adapter and model inference.

This document provides exhaustive technical details including:
- Exact database schemas and SQL queries
- Complete API request/response formats with JSON examples
- Precise code paths with function signatures
- State management details (frontend and backend)
- Error handling scenarios
- Configuration requirements
- Security implementation details
- Data structures and types
- Network protocols
- File formats
- Dependencies and prerequisites

## Adapters — Recent activity
- Window: rolling last 24 hours for the adapter, capped at the 6 newest entries.
- Event types: lineage history actions for the adapter (create/clone/promote/demote/load/unload/pin/unpin/duplicate, etc.) plus activation history points for that adapter.
- Source: adapter detail lineage and activation responses only (no telemetry stream events).
- Ordering: newest timestamp first; events outside the window are omitted; the UI falls back to “No recent events.” when none are available.

## Overview

The flow consists of three major phases:
1. **Authentication** - User login and session management
2. **Adapter Training** - Creating and training a new adapter
3. **Model Inference** - Using trained adapters for inference

## Prerequisites

### Database Setup
- SQLite database with WAL mode enabled
- All migrations applied (0001_init.sql through latest)
- Default tenant created: `id='default', name='default'`
- At least one user seeded in `users` table

### Configuration Files
- `configs/cp.toml` - Control plane configuration
- `configs/cp-8080.toml` - Server port configuration
- Environment variables: `VITE_API_URL` (frontend)

### Dependencies
**Backend:**
- `adapteros-server-api` - HTTP API server
- `adapteros-db` - Database layer
- `adapteros-core` - Core types and errors
- `adapteros-policy` - Policy enforcement
- `adapteros-lora-worker` - Inference worker
- `adapteros-lora-lifecycle` - Adapter lifecycle management
- `adapteros-orchestrator` - Training orchestration
- `adapteros-lora-router` - K-sparse adapter routing

**Frontend:**
- React 18+ with TypeScript
- `react-router-dom` - Routing
- `axios` or `fetch` - HTTP client
- State management (Context API or Zustand)

---

## Phase 1: Authentication & Login

### Frontend Implementation Details

#### 1. User Enters Credentials

**Component:** `ui/src/components/LoginForm.tsx`

**State Management:**
```typescript
const [email, setEmail] = useState<string>('');
const [password, setPassword] = useState<string>('');
const [error, setError] = useState<string | null>(null);
const [isLoading, setIsLoading] = useState<boolean>(false);
```

**Form Validation:**
- Email: Must match regex `/^[^\s@]+@[^\s@]+\.[^\s@]+$/`
- Password: Minimum length 1 character (no maximum in dev mode)
- Both fields required before submit enabled

**Code Path:** `ui/src/components/LoginForm.tsx:17-170`

#### 2. Submit Login Request

**API Client Method:** `ui/src/api/client.ts:292-299`

**Exact Request:**
```typescript
async login(credentials: types.LoginRequest): Promise<types.LoginResponse> {
  const response = await this.request<types.LoginResponse>('/v1/auth/login', {
    method: 'POST',
    body: JSON.stringify(credentials),
  });
  return response;
}
```

**Request Format:**
```json
POST /api/v1/auth/login
Content-Type: application/json
X-Request-ID: <computed-hash>

{
  "email": "user@example.com",
  "password": "password123"
}
```

**Request ID Computation:**
```typescript
private async computeRequestId(method: string, path: string, body: string): Promise<string> {
  const canonical = `${method}:${path}:${body}`;
  const encoder = new TextEncoder();
  const data = encoder.encode(canonical);
  const hashBuffer = await crypto.subtle.digest('SHA-256', data);
  const hashArray = Array.from(new Uint8Array(hashBuffer));
  return hashArray.map(b => b.toString(16).padStart(2, '0')).join('').substring(0, 32);
}
```

**Code Path:** `ui/src/api/client.ts:42-49, 292-299`

#### 3. Store Authentication Token

**Token Storage:**
- Token stored in **httpOnly cookie** by server (secure, not accessible to JavaScript)
- Response includes token in JSON body for legacy compatibility
- Frontend does NOT store token in localStorage (security best practice)

**Response Format:**
```json
{
  "token": "eyJhbGciOiJIUzI1NiIsInR5cCI6IkpXVCJ9...",
  "user_id": "01HZ3K5M7N9P1Q3R5S7T9U1V3W5",
  "role": "admin"
}
```

**HTTP Headers Set by Server:**
```
Set-Cookie: auth_token=<jwt>; HttpOnly; Secure; SameSite=Strict; Path=/; Max-Age=28800
```

**Code Path:** `crates/adapteros-server-api/src/handlers.rs:1240-1280`

#### 4. Fetch User Profile

**API Client Method:** `ui/src/api/client.ts:306-310`

**Request:**
```typescript
async getCurrentUser(): Promise<types.UserInfoResponse> {
  return this.request<types.UserInfoResponse>('/v1/auth/me');
}
```

**Response Format:**
```json
{
  "user_id": "01HZ3K5M7N9P1Q3R5S7T9U1V3W5",
  "email": "user@example.com",
  "role": "admin",
  "display_name": "John Doe",
  "tenant_id": "default",
  "permissions": ["read", "write", "admin"],
  "last_login_at": "2025-01-15T10:30:00Z",
  "mfa_enabled": false,
  "token_last_rotated_at": "2025-01-15T10:30:00Z"
}
```

**State Update in AuthProvider:**
```typescript
// ui/src/providers/CoreProviders.tsx:89-115
const refreshUser = useCallback(async () => {
  if (isRefreshingRef.current) return;
  isRefreshingRef.current = true;
  try {
    const userInfo = await apiClient.getCurrentUser();
    setUser({
      id: userInfo.user_id,
      email: userInfo.email,
      display_name: userInfo.display_name || userInfo.email,
      role: userInfo.role as User['role'],
      tenant_id: userInfo.tenant_id || '',
      permissions: userInfo.permissions || [],
      last_login_at: userInfo.last_login_at,
      mfa_enabled: userInfo.mfa_enabled,
      token_last_rotated_at: userInfo.token_last_rotated_at,
    });
  } catch (error) {
    setUser(null);
    logger.error('Failed to fetch user', { component: 'AuthProvider' }, toError(error));
  } finally {
    isRefreshingRef.current = false;
  }
}, []);
```

**Code Path:** `ui/src/providers/CoreProviders.tsx:89-115`

### Backend Implementation Details

#### 1. Receive Login Request

**Handler Function:** `crates/adapteros-server-api/src/handlers.rs:1145-1280`

**Function Signature:**
```rust
pub async fn auth_login(
    State(state): State<AppState>,
    Json(req): Json<LoginRequest>,
) -> Result<Response, (StatusCode, Json<ErrorResponse>)>
```

**Request Extraction:**
- Axum extracts JSON body into `LoginRequest` struct
- Struct definition: `crates/adapteros-api-types/src/auth.rs:8-11`
```rust
pub struct LoginRequest {
    pub email: String,
    pub password: String,
}
```

**Code Path:** `crates/adapteros-server-api/src/handlers.rs:1145-1148`

#### 2. Validate User

**Database Query:** `crates/adapteros-db/src/users.rs:79-87`

**Exact SQL Query:**
```sql
SELECT id, email, display_name, pw_hash, role, disabled, created_at 
FROM users 
WHERE email = ?
```

**Query Execution:**
```rust
pub async fn get_user_by_email(&self, email: &str) -> Result<Option<User>> {
    let user = sqlx::query_as::<_, User>(
        "SELECT id, email, display_name, pw_hash, role, disabled, created_at FROM users WHERE email = ?"
    )
    .bind(email)
    .fetch_optional(self.pool())
    .await?;
    Ok(user)
}
```

**Database Schema:**
```sql
CREATE TABLE IF NOT EXISTS users (
    id TEXT PRIMARY KEY,
    email TEXT UNIQUE NOT NULL,
    display_name TEXT NOT NULL,
    pw_hash TEXT NOT NULL,
    role TEXT NOT NULL CHECK(role IN ('admin','operator','sre','compliance','auditor','viewer')),
    disabled INTEGER NOT NULL DEFAULT 0,
    created_at TEXT NOT NULL DEFAULT (datetime('now'))
);
```

**Validation Logic:**
```rust
// crates/adapteros-server-api/src/handlers.rs:1152-1181
let user = state.db.get_user_by_email(&req.email).await
    .map_err(|e| {
        tracing::error!("Database error during user lookup: {}", e);
        anyhow_to_error_response(e, adapteros_core::AosError::Sqlx, None)
    })?
    .ok_or_else(|| {
        tracing::warn!("User not found: {}", req.email);
        to_error_response(
            adapteros_core::AosError::Validation("Invalid credentials".to_string()),
            None,
        )
    })?;

// Check if user is disabled
if user.disabled {
    return Err((
        StatusCode::FORBIDDEN,
        Json(ErrorResponse::new("user disabled").with_code("USER_DISABLED")),
    ));
}
```

**Code Path:** `crates/adapteros-server-api/src/handlers.rs:1152-1181`

#### 3. Verify Password

**Password Verification Logic:** `crates/adapteros-server-api/src/handlers.rs:1183-1234`

**Production Mode Check:**
```rust
let is_production = {
    let config = state.config.read().map_err(|_| {
        tracing::error!("Failed to read config for production mode check");
        config_lock_error_response(None)
    })?;
    config.production_mode
};
```

**Password Verification:**
```rust
let valid = if user.pw_hash == "password" {
    // Plain text password check only allowed when NOT in production mode
    if is_production {
        tracing::warn!("Plain text password attempted in production mode - rejecting");
        return Err((
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(
                ErrorResponse::new("authentication system misconfigured")
                    .with_code("CONFIG_ERROR")
                    .with_string_details("plain text passwords not allowed in production"),
            ),
        ));
    }
    tracing::debug!("Using plain text password check (development mode)");
    req.password == "password"
} else {
    // Use proper Argon2 verification
    tracing::debug!("Using Argon2 password verification");
    verify_password(&req.password, &user.pw_hash).map_err(|e| {
        tracing::error!("Password verification error: {}", e);
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(
                ErrorResponse::new("authentication error")
                    .with_code("AUTHENTICATION_ERROR")
                    .with_string_details(e.to_string()),
            ),
        )
    })?
};
```

**Argon2 Verification Function:** `crates/adapteros-server-api/src/auth.rs`
- Uses `argon2` crate for password hashing
- Parameters: `m_cost=19456`, `t_cost=2`, `p_cost=1`
- Hash format: `$argon2id$v=19$m=19456,t=2,p=1$<salt>$<hash>`

**Code Path:** `crates/adapteros-server-api/src/handlers.rs:1183-1234`

#### 4. Generate JWT Token

**Token Generation:** `crates/adapteros-server-api/src/handlers.rs:1238-1270`

**JWT Mode Selection:**
```rust
let token_result = match state.jwt_mode {
    JwtMode::EdDsa => {
        let keypair = state.crypto.clone_jwt_keypair();
        generate_token_ed25519(&user.id, &user.email, &user.role, "default", &keypair)
    }
    JwtMode::Hmac => {
        generate_token(&user.id, &user.email, &user.role, "default", &state.jwt_secret)
    }
};
```

**JWT Claims Structure:**
```rust
pub struct Claims {
    pub sub: String,        // User ID
    pub email: String,      // User email
    pub role: String,       // User role
    pub tenant_id: String,  // Tenant ID (default: "default")
    pub exp: i64,           // Expiration timestamp
    pub iat: i64,           // Issued at timestamp
    pub jti: String,        // JWT ID (UUID v4)
    pub nbf: i64,           // Not before timestamp
}
```

**Token Expiry:**
- Default: 8 hours (28800 seconds)
- Configurable via `auth.token_expiry_hours` in `configs/cp.toml`
- Expiry calculation: `now + Duration::hours(config.token_expiry_hours)`

**Token Generation Function (Ed25519):**
```rust
pub fn generate_token_ed25519(
    user_id: &str,
    email: &str,
    role: &str,
    tenant_id: &str,
    keypair: &ed25519_dalek::SigningKey,
) -> Result<String> {
    let now = Utc::now();
    let exp = (now + Duration::hours(8)).timestamp();
    let jti = Uuid::new_v4().to_string();
    
    let claims = Claims {
        sub: user_id.to_string(),
        email: email.to_string(),
        role: role.to_string(),
        tenant_id: tenant_id.to_string(),
        exp,
        iat: now.timestamp(),
        jti,
        nbf: now.timestamp(),
    };
    
    let header = jsonwebtoken::Header {
        alg: jsonwebtoken::Algorithm::EdDSA,
        ..Default::default()
    };
    
    jsonwebtoken::encode(
        &header,
        &claims,
        &jsonwebtoken::EncodingKey::from_ed_der(&keypair.to_bytes()),
    )
    .map_err(|e| AosError::Crypto(format!("Failed to encode JWT: {}", e)))
}
```

**Code Path:** `crates/adapteros-server-api/src/handlers.rs:1238-1270`, `crates/adapteros-server-api/src/auth.rs`

#### 5. Return Authentication Response

**Response Construction:** `crates/adapteros-server-api/src/handlers.rs:1270-1280`

**Response Format:**
```rust
let response = LoginResponse {
    token: token_result?,
    user_id: user.id.clone(),
    role: user.role.clone(),
};

Ok(Json(response))
```

**HTTP Response:**
```json
HTTP/1.1 200 OK
Content-Type: application/json
Set-Cookie: auth_token=<jwt>; HttpOnly; Secure; SameSite=Strict; Path=/; Max-Age=28800
X-Request-ID: <request-id>

{
  "token": "eyJhbGciOiJIUzI1NiIsInR5cCI6IkpXVCJ9...",
  "user_id": "01HZ3K5M7N9P1Q3R5S7T9U1V3W5",
  "role": "admin"
}
```

**Code Path:** `crates/adapteros-server-api/src/handlers.rs:1270-1280`

#### 6. Middleware Validation

**Middleware Function:** `crates/adapteros-server-api/src/middleware.rs:103-200`

**Token Extraction:**
```rust
pub async fn auth_middleware(
    State(state): State<AppState>,
    mut req: Request<axum::body::Body>,
    next: Next,
) -> Result<Response, (StatusCode, Json<ErrorResponse>)> {
    // Extract token from Authorization header, cookie, or query parameter
    let bearer_token = req.headers()
        .get(axum::http::header::AUTHORIZATION)
        .and_then(|header| header.to_str().ok())
        .and_then(|header| header.strip_prefix("Bearer "))
        .map(|token| token.to_string());
    
    let cookie_token = req.headers()
        .get(axum::http::header::COOKIE)
        .and_then(|header| header.to_str().ok())
        .and_then(|cookies| {
            cookies.split(';').find_map(|cookie| {
                let cookie = cookie.trim();
                if cookie.starts_with("auth_token=") {
                    Some(cookie.strip_prefix("auth_token=")?.to_string())
                } else {
                    None
                }
            })
        });
    
    let query_token = req.uri().query().and_then(|query| {
        form_urlencoded::parse(query.as_bytes())
            .find(|(key, _)| key == "token")
            .map(|(_, value)| value.into_owned())
    });
    
    if let Some(token) = bearer_token.or(cookie_token).or(query_token) {
        // Validate token based on JWT mode
        let claims_res = match state.jwt_mode {
            JwtMode::Hmac => validate_token(&token, &state.jwt_secret),
            JwtMode::EdDsa => {
                if let Some(ref pem) = state.jwt_public_key_pem {
                    validate_token_ed25519(&token, pem)
                } else {
                    let der = state.crypto.jwt_keypair.public_key().to_bytes();
                    validate_token_ed25519_der(&token, &der)
                }
            }
        };
        
        match claims_res {
            Ok(claims) => {
                req.extensions_mut().insert(claims);
                Ok(next.run(req).await)
            }
            Err(e) => {
                tracing::warn!(error = %e, "Token validation failed");
                Err((
                    StatusCode::UNAUTHORIZED,
                    Json(ErrorResponse::new("unauthorized").with_code("UNAUTHORIZED")),
                ))
            }
        }
    } else {
        Err((
            StatusCode::UNAUTHORIZED,
            Json(ErrorResponse::new("missing authentication token").with_code("UNAUTHORIZED")),
        ))
    }
}
```

**Claims Extraction in Handlers:**
```rust
pub async fn some_handler(
    Extension(claims): Extension<Claims>,
    // ... other parameters
) -> Result<Json<Response>> {
    // claims.sub - user ID
    // claims.email - user email
    // claims.role - user role
    // claims.tenant_id - tenant ID
    // ...
}
```

**Code Path:** `crates/adapteros-server-api/src/middleware.rs:103-200`

---

## Phase 2: Adapter Training

### Frontend Steps

1. **Navigate to training interface** (`ui/src/components/TrainingWizard.tsx`)
   - User selects "Train Adapter" from navigation
   - TrainingWizard component initialized

2. **Step 1: Select adapter category**
   - User chooses: `code`, `framework`, `codebase`, or `ephemeral`
   - Category determines available configuration options

3. **Step 2: Basic information**
   - User provides:
     - Adapter name
     - Description
     - Scope (global/tenant/user)

4. **Step 3: Data source selection**
   - User selects data source type:
     - Repository (Git repository)
     - Template (predefined training template)
     - Custom (upload JSONL file)
     - Directory (local directory path)
   - If repository: select from `apiClient.listRepositories()`
   - If template: select from `apiClient.listTrainingTemplates()`

5. **Step 4: Category-specific configuration**
   - **Code adapter**: Language, symbol targets
   - **Framework adapter**: Framework ID, version, API patterns
   - **Codebase adapter**: Repository scope, file patterns, exclude patterns
   - **Ephemeral adapter**: TTL seconds, context window

6. **Step 5: Training parameters**
   - User configures:
     - `rank`: LoRA rank (default: 8)
     - `alpha`: LoRA alpha (default: 16)
     - `targets`: LoRA target layers (e.g., `['q_proj', 'v_proj']`)
     - `epochs`: Number of training epochs (default: 3)
     - `learning_rate`: Learning rate (default: 3e-4)
     - `batch_size`: Batch size (default: 4)
     - Optional: `warmup_steps`, `max_seq_length`

7. **Step 6: Packaging & registration**
   - User selects:
     - Package adapter after training (`packageAfter`)
     - Register adapter in system (`registerAfter`)
     - Adapters root directory
     - Adapter ID (auto-generated if not provided)
     - Tier (default: 8 for ephemeral)

8. **Step 7: Review and submit**
   - User reviews all configuration
   - Submit training job: `apiClient.startTraining(request)`
   - POST to `/api/v1/training/start`

9. **Monitor training progress** (`ui/src/components/SingleFileAdapterTrainer.tsx:144`)
   - Poll training job status: `apiClient.getTrainingJob(jobId)`
   - GET `/api/v1/training/jobs/:job_id`
   - Display progress, metrics, logs
   - Update UI with job state: `queued`, `running`, `completed`, `failed`

### Backend Steps

1. **Receive training request** (`crates/adapteros-server-api/src/handlers.rs:10599`)
   - Handler: `start_training()`
   - Validate user permissions (Admin or Operator role required)

2. **Validate training parameters** (`crates/adapteros-server-api/src/handlers.rs:10612-10639`)
   - Validate directory_root is absolute if provided
   - Convert request config to internal `TrainingConfig`
   - Build `TrainingJobParams` using `TrainingJobBuilder`

3. **Create training job** (`crates/adapteros-server-api/src/handlers.rs:10641-10654`)
   - Call `training_service.start_training(params)`
   - Job created with status `queued` or `running`
   - Job ID generated and stored in database

4. **Emit activity event** (`crates/adapteros-server-api/src/handlers.rs:10656-10670`)
   - Log training start event
   - Store metadata: adapter_name, repo_id, template_id, etc.

5. **Create training session** (`crates/adapteros-server-api/src/handlers.rs:10672-10681`)
   - Store session metadata in memory
   - Link session to job ID

6. **Optional: Background registration** (`crates/adapteros-server-api/src/handlers.rs:10684-10753`)
   - If `register=true`, spawn background task
   - Wait for training job completion
   - Automatically register adapter when training completes
   - Poll job status every second (up to 2 hours timeout)

7. **Training execution** (`crates/adapteros-orchestrator/src/training.rs`)
   - Load training data from specified source
   - Tokenize training examples
   - Initialize MicroLoRA trainer
   - Execute training loop:
     - Forward pass
     - Loss calculation
     - Backward pass
     - Weight updates
   - Save trained adapter weights

8. **Package adapter** (if `package=true`)
   - Create `.aos` adapter package
   - Include manifest, weights, signatures
   - Save to adapters root directory

9. **Update job status**
   - Mark job as `completed` or `failed`
   - Store final metrics (loss, training time, etc.)
   - Update database records

10. **Training job status endpoint** (`crates/adapteros-server-api/src/handlers.rs:10572`)
    - Handler: `get_training_job()`
    - GET `/api/v1/training/jobs/:job_id`
    - Returns current job state, progress, metrics

---

## Phase 3: Model Inference

### Frontend Steps

1. **Navigate to inference playground** (`ui/src/components/InferencePlayground.tsx`)
   - User selects "Inference" from navigation
   - InferencePlayground component initialized

2. **Load available adapters** (`ui/src/components/InferencePlayground.tsx:135-177`)
   - Call `apiClient.listAdapters()`
   - GET `/api/v1/adapters`
   - Filter adapters by tenant
   - Display adapters with state: `hot`, `warm`, `resident`, `cold`, `unloaded`

3. **Select adapter** (optional)
   - User selects adapter from dropdown
   - Adapter ID stored in state
   - If no adapter selected, router selects adapters automatically

4. **Configure inference parameters**
   - User sets:
     - `prompt`: Input text prompt
     - `max_tokens`: Maximum tokens to generate (default: 100)
     - `temperature`: Sampling temperature (default: 0.7)
     - `top_k`: Top-k sampling (default: 50)
     - `top_p`: Nucleus sampling (default: 0.9)
     - `seed`: Optional random seed for determinism
     - `require_evidence`: Require citations/evidence

5. **Submit inference request** (`ui/src/components/InferencePlayground.tsx:202-239`)
   - Call `apiClient.infer(inferenceRequest)`
   - POST `/api/v1/infer`
   - Request includes: `prompt`, `adapters` (if selected), `max_tokens`, `temperature`, etc.

6. **Handle inference response**
   - Receive `InferResponse` with:
     - `response`: Generated text
     - `usage`: Token counts
     - `adapters_used`: List of adapter IDs used
     - `evidence`: Citations/evidence (if `require_evidence=true`)
   - Display response in UI
   - Save session to localStorage for history

7. **Streaming inference** (optional)
   - If streaming enabled, use `/api/v1/infer/stream`
   - Handle Server-Sent Events (SSE)
   - Update UI incrementally as tokens arrive

### Backend Steps

1. **Receive inference request** (`crates/adapteros-server-api/src/handlers.rs:4736`)
   - Handler: `infer()`
   - Extract request from JSON body
   - Validate prompt is not empty

2. **Policy enforcement** (`crates/adapteros-server-api/src/handlers.rs:4754-4868`)
   - Create `PolicyOperation` with request context
   - Call `policy_manager.enforce_policy()`
   - Check for policy violations
   - Block request if violations found (return 403)
   - Log violations if non-blocking

3. **Select worker** (`crates/adapteros-server-api/src/handlers.rs:4878-4900`)
   - Query database for available workers: `db.list_all_workers()`
   - Filter healthy workers
   - Select worker (round-robin or load-based)
   - Get worker UDS socket path

4. **Router selection** (if adapters not specified)
   - If `adapters` array not provided in request:
     - Extract prompt tokens
     - Call router: `router.select_adapters(request, k=3)`
     - Router uses Q15 quantized gates
     - Returns top K adapters based on prompt features

5. **Ensure adapters loaded** (`crates/adapteros-lora-lifecycle/src/lib.rs:1168`)
   - For each adapter ID:
     - Check adapter state in `LifecycleManager`
     - If `Unloaded`, trigger lazy loading
     - Load adapter weights into memory
     - Update adapter state to `Resident` or `Hot`

6. **Forward to worker** (`crates/adapteros-server-api/src/handlers.rs:4900+`)
   - Connect to worker UDS server
   - Send inference request:
     ```json
     {
       "cpid": tenant_id,
       "prompt": "...",
       "max_tokens": 100,
       "adapters": ["adapter_id_1", "adapter_id_2"],
       "require_evidence": false
     }
     ```
   - Wait for worker response

7. **Worker inference pipeline** (`crates/adapteros-lora-worker/src/inference_pipeline.rs:371`)
   - Check quarantine status
   - Apply chat template to prompt
   - Tokenize prompt: `tokenizer.encode(formatted_prompt)`
   - Validate sequence length

8. **Generation loop** (`crates/adapteros-lora-worker/src/inference_pipeline.rs`)
   - For each token to generate:
     - **Router decision**: Select adapters for this step (K-sparse routing)
     - **Load adapter weights**: Ensure selected adapters are in memory
     - **Forward pass**: 
       - Base model forward pass
       - LoRA adapter forward passes (for selected adapters)
       - Merge LoRA deltas with base weights
     - **Sample next token**: Use temperature/top-k/top-p sampling
     - **Append token**: Add to generated sequence
     - **Update state**: Track router decisions, evidence

9. **Return inference response**
   - Worker returns:
     ```json
     {
       "response": "generated text...",
       "usage": {
         "prompt_tokens": 12,
         "completion_tokens": 45,
         "total_tokens": 57
       },
       "adapters_used": ["adapter_id_1"],
       "evidence": [...],
       "router_decisions": [...]
     }
     ```

10. **Server response** (`crates/adapteros-server-api/src/handlers.rs`)
    - Format worker response as `InferResponse`
    - Return JSON to frontend
    - Log inference metrics

---

## Key API Endpoints

### Authentication
- `POST /api/v1/auth/login` - User login
- `GET /api/v1/auth/me` - Get current user
- `POST /api/v1/auth/logout` - Logout
- `POST /api/v1/auth/refresh` - Refresh session

### Training
- `POST /api/v1/training/start` - Start training job
- `GET /api/v1/training/jobs` - List training jobs
- `GET /api/v1/training/jobs/:job_id` - Get training job status
- `GET /api/v1/training/jobs/:job_id/logs` - Get training logs
- `POST /api/v1/training/jobs/:job_id/cancel` - Cancel training job

### Adapters
- `GET /api/v1/adapters` - List adapters
- `GET /api/v1/adapters/:adapter_id` - Get adapter details
- `POST /api/v1/adapters/register` - Register adapter
- `POST /api/v1/adapters/:adapter_id/unload` - Unload adapter

### Inference
- `POST /api/v1/infer` - Perform inference
- `POST /api/v1/infer/stream` - Streaming inference
- `POST /api/v1/infer/batch` - Batch inference

---

## Data Flow Summary

```
User Login
  ↓
[Frontend] LoginForm → apiClient.login()
  ↓
[Backend] auth_login() → Validate → Generate JWT
  ↓
[Frontend] Store token → Fetch user profile → Dashboard

Training Flow
  ↓
[Frontend] TrainingWizard → Configure → Submit
  ↓
[Backend] start_training() → Create job → Execute training
  ↓
[Backend] Training service → Train adapter → Package → Register
  ↓
[Frontend] Poll job status → Display progress → Complete

Inference Flow
  ↓
[Frontend] InferencePlayground → Select adapter → Configure → Submit
  ↓
[Backend] infer() → Policy check → Select worker → Forward request
  ↓
[Backend] Worker → Router selection → Load adapters → Generate tokens
  ↓
[Backend] Return response → [Frontend] Display results
```

---

---

## Error Handling Scenarios

### Authentication Errors

**Invalid Credentials (401):**
```json
{
  "error": "invalid credentials",
  "code": "INVALID_CREDENTIALS"
}
```
- **Frontend Handling:** Display error message, clear password field
- **Backend Location:** `crates/adapteros-server-api/src/handlers.rs:1228-1234`

**User Disabled (403):**
```json
{
  "error": "user disabled",
  "code": "USER_DISABLED"
}
```
- **Frontend Handling:** Display account disabled message
- **Backend Location:** `crates/adapteros-server-api/src/handlers.rs:1176-1181`

**Token Expired (401):**
```json
{
  "error": "unauthorized",
  "code": "UNAUTHORIZED"
}
```
- **Frontend Handling:** Trigger token refresh, retry request
- **Backend Location:** `crates/adapteros-server-api/src/middleware.rs:196-200`

### Training Errors

**Insufficient Permissions (403):**
```json
{
  "error": "insufficient permissions",
  "code": "FORBIDDEN"
}
```
- **Frontend Handling:** Show permission error, redirect to dashboard
- **Backend Location:** `crates/adapteros-server-api/src/handlers.rs:10604-10609`

**Invalid Training Parameters (400):**
```json
{
  "error": "invalid training parameters",
  "code": "BAD_REQUEST",
  "details": "directory_root must be absolute path"
}
```
- **Frontend Handling:** Highlight invalid fields, show validation errors
- **Backend Location:** `crates/adapteros-server-api/src/handlers.rs:10630-10639`

**Training Job Not Found (404):**
```json
{
  "error": "training job not found",
  "code": "NOT_FOUND"
}
```
- **Frontend Handling:** Show not found message, redirect to training list
- **Backend Location:** `crates/adapteros-server-api/src/handlers.rs:10576-10588`

### Inference Errors

**Policy Violation (403):**
```json
{
  "error": "policy violation",
  "code": "POLICY_VIOLATION",
  "details": {
    "request_id": "...",
    "tenant_id": "...",
    "violations": [...]
  }
}
```
- **Frontend Handling:** Display policy violation details, suggest remediation
- **Backend Location:** `crates/adapteros-server-api/src/handlers.rs:4830-4845`

**Worker Not Available (503):**
```json
{
  "error": "no workers available",
  "code": "SERVICE_UNAVAILABLE"
}
```
- **Frontend Handling:** Show retry option, display worker status
- **Backend Location:** `crates/adapteros-server-api/src/handlers.rs:4878-4886`

**Empty Prompt (400):**
```json
{
  "error": "prompt cannot be empty",
  "code": "INTERNAL_ERROR"
}
```
- **Frontend Handling:** Highlight prompt field, show validation error
- **Backend Location:** `crates/adapteros-server-api/src/handlers.rs:4742-4747`

---

## Configuration Details

### Backend Configuration (`configs/cp.toml`)

**Authentication Settings:**
```toml
[server]
production_mode = false  # Set to true in production
uds_socket = "/var/run/aos/default/aos.sock"

[auth]
mode = "development"  # Options: "development", "production", "mixed"
token_expiry_hours = 8
jwt_mode = "hmac"  # Options: "hmac", "eddsa"
```

**Database Settings:**
```toml
[database]
path = "var/adapteros.db"
wal_mode = true
```

**Training Settings:**
```toml
[training]
default_rank = 8
default_alpha = 16
default_epochs = 3
default_learning_rate = 0.0003
default_batch_size = 4
adapters_root = "./adapters"
```

**Worker Settings:**
```toml
[worker]
uds_path = "/var/run/aos/{tenant}/aos.sock"
timeout_seconds = 30
max_memory_headroom_pct = 15.0
```

### Frontend Configuration

**Environment Variables:**
```bash
VITE_API_URL=http://localhost:8080/api  # API base URL
VITE_ENABLE_DEV_BYPASS=true  # Enable dev token bypass (dev only)
```

**API Client Configuration:**
```typescript
// ui/src/api/client.ts
const API_BASE_URL = (import.meta as { env?: { VITE_API_URL?: string } }).env?.VITE_API_URL || '/api';

const retryConfig = {
  maxAttempts: 3,
  baseDelay: 1000,
  maxDelay: 10000,
  backoffMultiplier: 2,
  jitter: 0.1,
};
```

---

## Database Schema Reference

### Users Table
```sql
CREATE TABLE IF NOT EXISTS users (
    id TEXT PRIMARY KEY,
    email TEXT UNIQUE NOT NULL,
    display_name TEXT NOT NULL,
    pw_hash TEXT NOT NULL,
    role TEXT NOT NULL CHECK(role IN ('admin','operator','sre','compliance','auditor','viewer')),
    disabled INTEGER NOT NULL DEFAULT 0,
    created_at TEXT NOT NULL DEFAULT (datetime('now'))
);
```

### Training Jobs Table
```sql
CREATE TABLE IF NOT EXISTS repository_training_jobs (
    id TEXT PRIMARY KEY,
    repo_id TEXT NOT NULL,
    training_config_json TEXT NOT NULL,
    status TEXT NOT NULL CHECK(status IN ('pending','running','completed','failed','cancelled')),
    progress_json TEXT NOT NULL,
    started_at TEXT,
    completed_at TEXT,
    created_by TEXT NOT NULL,
    adapter_name TEXT,
    template_id TEXT,
    created_at TEXT DEFAULT (datetime('now')),
    metadata_json TEXT
);
```

### Adapters Table
```sql
CREATE TABLE IF NOT EXISTS adapters (
    id TEXT PRIMARY KEY,
    tenant_id TEXT NOT NULL REFERENCES tenants(id) ON DELETE CASCADE,
    name TEXT NOT NULL,
    tier TEXT NOT NULL CHECK(tier IN ('persistent','warm','ephemeral')),
    hash_b3 TEXT UNIQUE NOT NULL,
    rank INTEGER NOT NULL,
    alpha REAL NOT NULL,
    targets_json TEXT NOT NULL,
    acl_json TEXT,
    adapter_id TEXT,
    languages_json TEXT,
    framework TEXT,
    active INTEGER NOT NULL DEFAULT 1,
    created_at TEXT NOT NULL DEFAULT (datetime('now')),
    updated_at TEXT NOT NULL DEFAULT (datetime('now')),
    UNIQUE(tenant_id, name)
);
```

### Workers Table
```sql
CREATE TABLE IF NOT EXISTS workers (
    id TEXT PRIMARY KEY,
    tenant_id TEXT NOT NULL REFERENCES tenants(id) ON DELETE CASCADE,
    node_id TEXT NOT NULL REFERENCES nodes(id) ON DELETE CASCADE,
    plan_id TEXT NOT NULL REFERENCES plans(id),
    uds_path TEXT NOT NULL,
    pid INTEGER,
    status TEXT NOT NULL DEFAULT 'starting' CHECK(status IN ('starting','serving','draining','stopped','crashed')),
    memory_headroom_pct REAL,
    k_current INTEGER,
    adapters_loaded_json TEXT,
    started_at TEXT NOT NULL DEFAULT (datetime('now')),
    last_heartbeat_at TEXT
);
```

---

## API Request/Response Formats

### Training Start Request
```json
POST /api/v1/training/start
Authorization: Bearer <token>
Content-Type: application/json

{
  "adapter_name": "my_code_adapter",
  "config": {
    "rank": 8,
    "alpha": 16,
    "targets": ["q_proj", "v_proj"],
    "epochs": 3,
    "learning_rate": 0.0003,
    "batch_size": 4,
    "warmup_steps": null,
    "max_seq_length": null
  },
  "template_id": null,
  "repo_id": "repo_123",
  "dataset_path": null,
  "directory_root": "/absolute/path/to/repo",
  "directory_path": "src",
  "tenant_id": "default",
  "adapters_root": "./adapters",
  "package": true,
  "register": true,
  "adapter_id": "my_code_adapter_v1",
  "tier": 8
}
```

### Training Job Response
```json
{
  "id": "train_20250115_abc123",
  "adapter_name": "my_code_adapter",
  "template_id": null,
  "repo_id": "repo_123",
  "status": "running",
  "progress_pct": 45.5,
  "current_epoch": 1,
  "total_epochs": 3,
  "current_loss": 2.145,
  "learning_rate": 0.0003,
  "tokens_per_second": 1250.5,
  "created_at": "2025-01-15T10:30:00Z",
  "started_at": "2025-01-15T10:30:05Z",
  "completed_at": null,
  "error_message": null,
  "estimated_completion": "2025-01-15T14:30:00Z",
  "artifact_path": null,
  "adapter_id": null,
  "weights_hash_b3": null
}
```

### Inference Request
```json
POST /api/v1/infer
Authorization: Bearer <token>
Content-Type: application/json

{
  "prompt": "Write a Python function to calculate fibonacci numbers",
  "max_tokens": 200,
  "temperature": 0.7,
  "top_k": 50,
  "top_p": 0.9,
  "seed": null,
  "require_evidence": false,
  "adapters": ["code_lang_v1"]
}
```

### Inference Response
```json
{
  "text": "def fibonacci(n):\n    if n <= 1:\n        return n\n    return fibonacci(n-1) + fibonacci(n-2)",
  "tokens": [1234, 5678, 9012],
  "finish_reason": "stop",
  "trace": {
    "adapters_used": ["code_lang_v1"],
    "router_decisions": [
      {
        "position": 0,
        "adapter_ids": [1],
        "gates": [32767]
      }
    ],
    "latency_ms": 1250
  }
}
```

---

## Network Protocols

### HTTP/HTTPS
- **Protocol:** HTTP/1.1 or HTTP/2
- **TLS:** Required in production (HTTPS)
- **Content-Type:** `application/json` for all JSON requests/responses
- **Authentication:** Bearer token in `Authorization` header or `auth_token` cookie

### Unix Domain Sockets (UDS)
- **Protocol:** HTTP over Unix Domain Socket
- **Path Format:** `/var/run/aos/{tenant_id}/aos.sock`
- **Client:** `adapteros_client::UdsClient`
- **Timeout:** 30 seconds default
- **Used For:** Worker communication (inference requests)

---

## File Formats

### Adapter Package (.aos)
- **Format:** Binary archive with 64-byte header (AOS 3.0)
- **Structure:**
  ```
  adapter.aos (AOS 3.0 binary)
  +--------+--------+------------------------------------------+
  | Offset | Size   | Field                                    |
  +--------+--------+------------------------------------------+
  | 0      | 8      | Magic bytes: "AOS3\x00\x00\x00\x00"      |
  | 8      | 4      | Format version (u32 LE) = 3              |
  | 12     | 4      | Flags (reserved)                         |
  | 16     | 8      | Total file size (u64 LE)                 |
  | 24     | 8      | Weights offset (u64 LE)                  |
  | 32     | 8      | Weights size (u64 LE)                    |
  | 40     | 8      | Manifest offset (u64 LE)                 |
  | 48     | 8      | Manifest size (u64 LE)                   |
  | 56     | 8      | Reserved                                 |
  +--------+--------+------------------------------------------+
  | 64     | N      | Weights (SafeTensors or Q15)             |
  | 64+N   | M      | Manifest (JSON metadata)                 |
  +--------+--------+------------------------------------------+
  ```
- **See:** [AOS Format Specification](AOS_FORMAT.md) for full details

### Training Data (JSONL)
- **Format:** JSON Lines (one JSON object per line)
- **Schema:**
  ```json
  {
    "id": "example_1",
    "prompt": "User instruction",
    "response": "Desired response",
    "weight": 1.0,
    "metadata": {
      "category": "code",
      "tags": ["python", "function"]
    }
  }
  ```

---

## State Management

### Frontend State (React Context)

**AuthProvider State:**
```typescript
interface AuthContextValue {
  user: User | null;
  isLoading: boolean;
  login: (credentials: LoginRequest) => Promise<void>;
  logout: () => Promise<void>;
  refreshUser: () => Promise<void>;
  refreshSession: () => Promise<void>;
}
```

**Training Wizard State:**
```typescript
interface WizardState {
  currentStep?: number;
  category: AdapterCategory | null;
  name: string;
  description: string;
  scope: AdapterScope;
  dataSourceType: 'repository' | 'template' | 'custom' | 'directory';
  // ... more fields
}
```

### Backend State (AppState)

**AppState Structure:**
```rust
pub struct AppState {
    pub db: Db,
    pub config: Arc<RwLock<Config>>,
    pub jwt_mode: JwtMode,
    pub jwt_secret: String,
    pub jwt_public_key_pem: Option<String>,
    pub crypto: Arc<CryptoState>,
    pub policy_manager: Arc<PolicyManager>,
    pub training_service: Arc<TrainingService>,
    pub training_sessions: Arc<RwLock<HashMap<String, TrainingSessionMetadata>>>,
}
```

---

## References

### Code Locations

**Authentication:**
- Handler: `crates/adapteros-server-api/src/handlers.rs:1145-1280`
- Middleware: `crates/adapteros-server-api/src/middleware.rs:103-200`
- Auth Utils: `crates/adapteros-server-api/src/auth.rs`
- Database: `crates/adapteros-db/src/users.rs`

**Training:**
- Handler: `crates/adapteros-server-api/src/handlers.rs:10599-10756`
- Service: `crates/adapteros-orchestrator/src/training.rs`
- Database: `crates/adapteros-db/src/training_jobs.rs`
- Frontend: `ui/src/components/TrainingWizard.tsx`

**Inference:**
- Handler: `crates/adapteros-server-api/src/handlers.rs:4736-5000+`
- Worker Pipeline: `crates/adapteros-lora-worker/src/inference_pipeline.rs`
- Router: `crates/adapteros-lora-router/src/lib.rs`
- Lifecycle: `crates/adapteros-lora-lifecycle/src/lib.rs`
- Frontend: `ui/src/components/InferencePlayground.tsx`

**API Client:**
- Frontend: `ui/src/api/client.ts`
- Types: `crates/adapteros-api-types/src/`
- State Management: `ui/src/providers/CoreProviders.tsx`

### Database Migrations
- Initial Schema: `crates/adapteros-db/migrations/0001_init.sql`
- Training Jobs: `crates/adapteros-db/migrations/0013_git_repository_integration.sql`
- Training Extensions: `crates/adapteros-db/migrations/0050_training_jobs_extensions.sql`
- Training Datasets: `crates/adapteros-db/migrations/0041_training_datasets.sql`

### Configuration Files
- Control Plane: `configs/cp.toml`
- Server Port: `configs/cp-8080.toml`
- Production: `configs/production-multinode.toml`

MLNavigator Inc 2025-12-08.
