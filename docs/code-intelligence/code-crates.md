# Code Intelligence Crate Structure

## Overview

Four new crates extend AdapterOS with code intelligence capabilities. Each crate has clear responsibilities and integrates with existing infrastructure.

---

## New Crates

### 1. aos-codegraph

**Purpose**: CodeGraph building, tree-sitter parsing, symbol extraction.

**Location**: `crates/aos-codegraph/`

**Dependencies**:
```toml
[dependencies]
aos-core = { path = "../aos-core" }
tree-sitter = "0.20"
tree-sitter-python = "0.20"
tree-sitter-rust = "0.20"
tree-sitter-typescript = "0.20"
tree-sitter-go = "0.19"
tree-sitter-java = "0.19"
blake3 = "1.5"
serde = { version = "1.0", features = ["derive"] }
serde_json = "1.0"
bincode = "1.3"
walkdir = "2"
thiserror = "1.0"
```

**Public API**:
```rust
// Graph building
pub struct CodeGraphBuilder;
impl CodeGraphBuilder {
    pub fn new(repo_id: &str, commit: &str) -> Self;
    pub fn scan_directory(&mut self, path: &Path, languages: &[Language]) -> Result<()>;
    pub fn detect_frameworks(&mut self) -> Result<()>;
    pub fn build_call_graph(&mut self) -> Result<()>;
    pub fn map_tests(&mut self) -> Result<()>;
    pub fn build(self) -> Result<CodeGraph>;
}

// Graph structure
pub struct CodeGraph {
    pub repo_id: String,
    pub commit_sha: String,
    pub files: Vec<FileNode>,
    pub symbols: Vec<SymbolNode>,
    pub tests: Vec<TestNode>,
    pub frameworks: Vec<FrameworkNode>,
    pub edges: Vec<Edge>,
}

impl CodeGraph {
    pub fn hash(&self) -> B3Hash;
    pub fn to_binary(&self) -> Result<CodeGraphBinary>;
    pub fn from_binary(bytes: &[u8]) -> Result<Self>;
    pub fn symbols_in_file(&self, file_id: &FileId) -> impl Iterator<Item = &SymbolNode>;
    pub fn callers_of(&self, symbol_id: &SymbolId) -> Vec<SymbolId>;
    pub fn tests_covering(&self, symbol_id: &SymbolId) -> Vec<TestId>;
    pub fn compute_test_impact(&self, files: &[FileId]) -> Result<Vec<TestId>>;
}

// Parsing
pub struct LanguageParser;
impl LanguageParser {
    pub fn new(lang: LanguageConfig) -> Result<Self>;
    pub fn parse(&mut self, source: &str) -> Result<Tree>;
}

// Symbol extraction
pub fn extract_symbols(file: &FileNode, source: &str) -> Result<Vec<SymbolNode>>;
pub fn extract_calls(file: &FileNode, source: &str) -> Result<Vec<CallEdge>>;
pub fn extract_imports(file: &FileNode, source: &str) -> Result<Vec<ImportEdge>>;

// Framework detection
pub fn detect_frameworks(repo_path: &Path) -> Result<Vec<Framework>>;
```

**Modules**:
- `lib.rs`: Public API
- `graph.rs`: CodeGraph structure and methods
- `parser.rs`: Tree-sitter wrapper
- `symbols.rs`: Symbol extraction
- `calls.rs`: Call graph building
- `imports.rs`: Import analysis
- `tests.rs`: Test mapping
- `frameworks.rs`: Framework detection
- `serialization.rs`: Binary format

---

### 2. aos-codepolicy

**Purpose**: Code-specific policy validation, patch safety checks.

**Location**: `crates/aos-codepolicy/`

**Dependencies**:
```toml
[dependencies]
aos-core = { path = "../aos-core" }
aos-policy = { path = "../aos-policy" }
regex = "1.10"
serde = { version = "1.0", features = ["derive"] }
serde_json = "1.0"
glob = "0.3"
thiserror = "1.0"
```

**Public API**:
```rust
// Policy definition
pub struct CodePolicy {
    pub evidence_min_spans: usize,
    pub allow_auto_apply: bool,
    pub require_test_coverage: Option<f32>,
    pub path_allowlist: Vec<String>,
    pub path_denylist: Vec<String>,
    pub allow_external_deps: bool,
    pub secret_patterns: Vec<String>,
    pub max_patch_size_lines: usize,
    pub forbidden_operations: Vec<String>,
}

// Policy engine
pub struct CodePolicyEngine;
impl CodePolicyEngine {
    pub fn new(policy: CodePolicy) -> Self;
    
    pub fn validate_path(&self, path: &Path) -> Result<PathValidation>;
    pub fn scan_secrets(&self, content: &str) -> Result<Vec<SecretDetection>>;
    pub fn scan_forbidden_ops(&self, content: &str) -> Result<Vec<ForbiddenOpDetection>>;
    pub fn validate_patch(&self, patch: &PatchSet) -> Result<PatchValidation>;
}

// Validation results
pub struct PathValidation {
    pub allowed: bool,
    pub reason: String,
    pub matched_pattern: Option<String>,
}

pub struct SecretDetection {
    pub line: usize,
    pub column: usize,
    pub pattern: String,
    pub matched_text: String,
    pub severity: Severity,
}

pub struct PatchValidation {
    pub safe_to_apply: bool,
    pub violations: Vec<Violation>,
    pub warnings: Vec<Warning>,
    pub checks: PolicyChecks,
}

pub struct PolicyChecks {
    pub path_allowed: bool,
    pub no_secrets: bool,
    pub no_forbidden_ops: bool,
    pub size_ok: bool,
    pub no_migrations: bool,
}
```

**Modules**:
- `lib.rs`: Public API
- `policy.rs`: Policy definition and engine
- `paths.rs`: Path validation
- `secrets.rs`: Secret detection
- `operations.rs`: Forbidden operation detection
- `patches.rs`: Patch validation

---

### 3. aos-codejobs

**Purpose**: Background jobs for scanning, indexing, CDP creation, ephemeral training.

**Location**: `crates/aos-codejobs/`

**Dependencies**:
```toml
[dependencies]
aos-core = { path = "../aos-core" }
aos-codegraph = { path = "../aos-codegraph" }
aos-artifacts = { path = "../aos-artifacts" }
aos-registry = { path = "../aos-registry" }
tokio = { version = "1.35", features = ["full"] }
serde = { version = "1.0", features = ["derive"] }
serde_json = "1.0"
rusqlite = { version = "0.30", features = ["bundled"] }
thiserror = "1.0"
tracing = "0.1"
uuid = { version = "1.6", features = ["v4"] }
```

**Public API**:
```rust
// Job system
pub struct JobQueue;
impl JobQueue {
    pub fn new() -> Self;
    pub async fn submit<J: Job>(&self, job: J) -> Result<JobId>;
    pub async fn status(&self, job_id: &JobId) -> Result<JobStatus>;
    pub async fn cancel(&self, job_id: &JobId) -> Result<()>;
}

pub trait Job: Send + Sync {
    async fn execute(&self) -> Result<JobResult>;
    fn name(&self) -> &str;
    fn estimated_duration(&self) -> Duration;
}

// Scan job
pub struct ScanJob {
    pub tenant_id: String,
    pub repo_id: String,
    pub commit: String,
    pub options: ScanOptions,
}

impl Job for ScanJob {
    async fn execute(&self) -> Result<JobResult> {
        // 1. Parse repository
        // 2. Build CodeGraph
        // 3. Create indices
        // 4. Store in CAS
        // 5. Update registry
    }
}

// CDP job
pub struct CommitDeltaJob {
    pub tenant_id: String,
    pub repo_id: String,
    pub commit: String,
    pub parent: String,
    pub options: CdpOptions,
}

impl Job for CommitDeltaJob {
    async fn execute(&self) -> Result<JobResult> {
        // 1. git diff
        // 2. Extract changed symbols
        // 3. Run tests (optional)
        // 4. Run linter (optional)
        // 5. Create CDP
        // 6. Store in CAS
    }
}

// Ephemeral training job
pub struct EphemeralTrainJob {
    pub tenant_id: String,
    pub adapter_id: String,
    pub cdp_id: String,
    pub config: EphemeralConfig,
}

impl Job for EphemeralTrainJob {
    async fn execute(&self) -> Result<JobResult> {
        // 1. Load CDP
        // 2. Generate training pairs
        // 3. Train micro-LoRA (rank 4-8)
        // 4. Package adapter
        // 5. Store in CAS
        // 6. Register with TTL
    }
}

// Job status
pub struct JobStatus {
    pub job_id: JobId,
    pub status: Status,
    pub progress: Progress,
    pub result: Option<JobResult>,
    pub error: Option<String>,
    pub started_at: DateTime<Utc>,
    pub completed_at: Option<DateTime<Utc>>,
}

pub enum Status {
    Pending,
    Running,
    Completed,
    Failed,
    Cancelled,
}
```

**Modules**:
- `lib.rs`: Public API
- `queue.rs`: Job queue implementation
- `scan.rs`: Scan job
- `cdp.rs`: CDP creation job
- `ephemeral.rs`: Ephemeral training job
- `status.rs`: Job status tracking

---

### 4. aos-codeapi

**Purpose**: DTOs, request/response types, API handlers for code endpoints.

**Location**: `crates/aos-codeapi/`

**Dependencies**:
```toml
[dependencies]
aos-core = { path = "../aos-core" }
aos-codegraph = { path = "../aos-codegraph" }
aos-codepolicy = { path = "../aos-codepolicy" }
aos-codejobs = { path = "../aos-codejobs" }
aos-registry = { path = "../aos-registry" }
axum = "0.7"
serde = { version = "1.0", features = ["derive"] }
serde_json = "1.0"
tokio = { version = "1.35", features = ["full"] }
thiserror = "1.0"
validator = "0.16"
```

**Public API**:
```rust
// Request DTOs
#[derive(Deserialize, Validate)]
pub struct RegisterRepoRequest {
    pub tenant_id: String,
    pub repo_id: String,
    #[validate(custom = "validate_path")]
    pub path: String,
    pub languages: Vec<String>,
    pub default_branch: Option<String>,
}

#[derive(Deserialize, Validate)]
pub struct ScanRequest {
    pub tenant_id: String,
    pub repo_id: String,
    pub commit: String,
    pub full_scan: bool,
}

#[derive(Deserialize, Validate)]
pub struct CommitDeltaRequest {
    pub tenant_id: String,
    pub repo_id: String,
    pub commit: String,
    pub parent: String,
    pub options: CdpOptions,
}

#[derive(Deserialize, Validate)]
pub struct PatchProposeRequest {
    pub tenant_id: String,
    pub repo_id: String,
    pub commit: String,
    pub request: PatchRequest,
    pub options: PatchOptions,
}

// Response DTOs
#[derive(Serialize)]
pub struct ScanResponse {
    pub status: String,
    pub job_id: String,
    pub repo_id: String,
    pub commit: String,
    pub estimated_duration_seconds: u64,
}

#[derive(Serialize)]
pub struct PatchProposeResponse {
    pub patch_set_id: String,
    pub status: String,
    pub patches: Vec<Patch>,
    pub rationale: String,
    pub citations: Vec<Citation>,
    pub trace: Trace,
}

// API handlers
pub mod handlers {
    pub async fn register_repo(req: RegisterRepoRequest) -> Result<Response>;
    pub async fn scan(req: ScanRequest) -> Result<ScanResponse>;
    pub async fn commit_delta(req: CommitDeltaRequest) -> Result<Response>;
    pub async fn patch_propose(req: PatchProposeRequest) -> Result<PatchProposeResponse>;
    pub async fn patch_apply(req: PatchApplyRequest) -> Result<Response>;
}

// Response builders
pub struct ResponseBuilder;
impl ResponseBuilder {
    pub fn with_citations(citations: Vec<Citation>) -> Self;
    pub fn with_trace(trace: Trace) -> Self;
    pub fn build(self) -> PatchProposeResponse;
}
```

**Modules**:
- `lib.rs`: Public API
- `requests.rs`: Request DTOs
- `responses.rs`: Response DTOs
- `handlers/`: API handlers
  - `registry.rs`
  - `scan.rs`
  - `ephemeral.rs`
  - `patch.rs`
  - `security.rs`
- `validation.rs`: Request validation
- `builders.rs`: Response builders

---

## Integration with Existing Crates

### aos-registry
- Extended schema for code tables
- Queries for repos, graphs, indices

### aos-artifacts
- Store CodeGraphs, indices, CDPs
- Retrieve by hash

### aos-worker
- Load indices on startup
- Inject code features into router
- Use symbol/vector search for evidence

### aos-router
- New feature extractors (lang, framework, symbol_hits, etc.)
- Code-specific scoring functions

### aos-policy
- Load code policies from manifest
- Enforce during patch propose/apply

### aos-telemetry
- Log code events (scan, train, patch)
- Track adapter activation patterns

### aos-cli
- New subcommands for code operations
- Job status monitoring

---

## Build & Test

### Workspace Cargo.toml

Add new crates to workspace:

```toml
[workspace]
members = [
    # Existing crates...
    "crates/aos-codegraph",
    "crates/aos-codepolicy",
    "crates/aos-codejobs",
    "crates/aos-codeapi",
]
```

### Build

```bash
cargo build --package aos-codegraph
cargo build --package aos-codepolicy
cargo build --package aos-codejobs
cargo build --package aos-codeapi
```

### Test

```bash
cargo test --package aos-codegraph
cargo test --package aos-codepolicy
cargo test --package aos-codejobs
cargo test --package aos-codeapi
```

### Integration Tests

```bash
# Test full pipeline
cargo test --test code_intelligence_integration

# Test with real repo
cargo test --test code_scan_real_repo -- --ignored
```

---

## Documentation

Each crate includes:
- `README.md`: Overview and examples
- `ARCHITECTURE.md`: Internal design
- Inline docs: `cargo doc --package aos-codegraph --open`

---

## Size Estimates

| Crate          | Lines of Code | Binary Size | Compile Time |
|----------------|---------------|-------------|--------------|
| aos-codegraph  | ~3K LOC       | ~2 MB       | ~15s         |
| aos-codepolicy | ~1K LOC       | ~500 KB     | ~5s          |
| aos-codejobs   | ~2K LOC       | ~1 MB       | ~10s         |
| aos-codeapi    | ~2K LOC       | ~1.5 MB     | ~12s         |
| **Total**      | **~8K LOC**   | **~5 MB**   | **~42s**     |

These add ~15% to total codebase size and compile time.
