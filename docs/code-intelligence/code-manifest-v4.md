# Manifest V4: Code Intelligence Extensions

## Overview

Manifest V4 extends the existing V3 schema with **backward-compatible** metadata for code intelligence. Existing fields are preserved; new fields are additive.

## Schema Changes

### Adapter Extensions

```yaml
adapters:
  - id: string
    hash: B3Hash
    tier: "persistent" | "ephemeral"  # EXISTING (lifecycle)
    rank: u32
    alpha: f32
    target_modules: string[]
    acl: string[]
    ttl: u32?                          # EXISTING (ephemeral only)
    
    # NEW FIELDS (code intelligence)
    category: "code" | "framework" | "codebase" | "ephemeral"  # Semantic role
    scope: "global" | "tenant" | "repo" | "commit"             # Boundary
    framework_id: string?              # e.g., "django", "react"
    framework_version: string?         # e.g., "4.2", "18"
    repo_id: string?                   # e.g., "acme/payments"
    commit_sha: string?                # e.g., "ab12cd34" (ephemeral only)
    intent: string?                    # e.g., "testing", "security", "refactor"
    metadata: object?                  # Extensible metadata
```

### Router Extensions

```yaml
router:
  k_sparse: usize
  gate_quant: string
  entropy_floor: f32
  tau: f32
  sample_tokens_full: usize
  
  # NEW FIELDS (code-specific features)
  features: string[]                   # Feature names to compute
  code_features:                       # Code-specific configuration
    enable_lang_detection: bool
    enable_framework_prior: bool
    enable_symbol_hits: bool
    enable_path_tokens: bool
    enable_commit_hint: bool
    max_framework_adapters: usize      # Default: 1
```

### Policy Extensions

```yaml
policies:
  # EXISTING policies
  egress: object
  access: object
  evidence: object
  # ...
  
  # NEW: Code-specific policies
  code:
    evidence_min_spans: usize          # Minimum code/test/doc spans (default: 1)
    allow_auto_apply: bool             # Auto-apply patches (default: false)
    require_test_coverage: f32?        # Min coverage for auto-apply
    path_allowlist: string[]           # Allowed paths for patches
    path_denylist: string[]            # Forbidden paths
    allow_external_deps: bool          # Allow dependency suggestions
    secret_patterns: string[]          # Regex patterns for secret detection
    max_patch_size_lines: usize        # Max patch size (default: 500)
```

## Example Manifests

### Example 1: Base + Code + Framework

```yaml
schema: "adapteros.manifest.v4"

base:
  model_id: "Qwen2.5-7B-Instruct"
  model_hash: "b3:a1b2c3d4..."
  arch: "qwen2"
  vocab_size: 152064
  hidden_dim: 3584
  n_layers: 28
  n_heads: 28
  config_hash: "b3:..."
  tokenizer_hash: "b3:..."
  tokenizer_cfg_hash: "b3:..."

adapters:
  # Code tier (domain, global)
  - id: "code_lang_v1"
    hash: "b3:e5f6g7h8..."
    tier: "persistent"
    category: "code"
    scope: "global"
    rank: 16
    alpha: 32
    target_modules:
      - "q_proj"
      - "k_proj"
      - "v_proj"
      - "o_proj"
      - "gate_proj"
      - "up_proj"
      - "down_proj"
  
  # Framework tier (type, global)
  - id: "framework_django_v1"
    hash: "b3:i9j0k1l2..."
    tier: "persistent"
    category: "framework"
    scope: "global"
    framework_id: "django"
    framework_version: "4.2"
    rank: 12
    alpha: 24
    target_modules:
      - "q_proj"
      - "k_proj"
      - "v_proj"
      - "o_proj"
      - "gate_proj"
      - "up_proj"
      - "down_proj"
  
  - id: "framework_react_v2"
    hash: "b3:m3n4o5p6..."
    tier: "persistent"
    category: "framework"
    scope: "global"
    framework_id: "react"
    framework_version: "18"
    rank: 12
    alpha: 24
    target_modules:
      - "q_proj"
      - "k_proj"
      - "v_proj"
      - "o_proj"
      - "gate_proj"
      - "up_proj"
      - "down_proj"

router:
  k_sparse: 3
  gate_quant: "q15"
  entropy_floor: 0.02
  tau: 1.0
  sample_tokens_full: 128
  features:
    - "lang_one_hot"
    - "framework_prior"
    - "symbol_hits"
    - "path_tokens"
    - "attn_entropy"
  code_features:
    enable_lang_detection: true
    enable_framework_prior: true
    enable_symbol_hits: true
    enable_path_tokens: true
    enable_commit_hint: false
    max_framework_adapters: 1

telemetry:
  schema_hash: "b3:q7r8s9t0..."
  sampling:
    token: 0.05
    router: 1.0
    inference: 1.0
  router_full_tokens: 128
  bundle:
    max_events: 500000
    max_bytes: 268435456

policies:
  egress:
    mode: "deny_all"
  access:
    adapters: "RBAC"
    datasets: "ABAC"
  evidence:
    require_open_book: true
    min_spans: 1
  code:
    evidence_min_spans: 1
    allow_auto_apply: false
    path_allowlist:
      - "src/**"
      - "lib/**"
    path_denylist:
      - "**/.env"
      - "**/secrets/**"
      - ".github/**"
    allow_external_deps: false
    secret_patterns:
      - "(?i)(api[_-]?key|password|secret|token)\\s*=\\s*['\"][^'\"]+['\"]"
    max_patch_size_lines: 500

seeds:
  global: "b3:u1v2w3x4..."
```

### Example 2: Codebase + Ephemeral

```yaml
schema: "adapteros.manifest.v4"

base:
  model_id: "Qwen2.5-7B-Instruct"
  model_hash: "b3:a1b2c3d4..."
  arch: "qwen2"
  vocab_size: 152064
  hidden_dim: 3584
  n_layers: 28
  n_heads: 28
  config_hash: "b3:..."
  tokenizer_hash: "b3:..."
  tokenizer_cfg_hash: "b3:..."

adapters:
  # Code tier
  - id: "code_lang_v1"
    hash: "b3:e5f6g7h8..."
    tier: "persistent"
    category: "code"
    scope: "global"
    rank: 16
    alpha: 32
    target_modules: ["q_proj","k_proj","v_proj","o_proj","gate_proj","up_proj","down_proj"]
  
  # Codebase tier (tenant-specific)
  - id: "codebase_acme_payments_v7"
    hash: "b3:y5z6a7b8..."
    tier: "persistent"
    category: "codebase"
    scope: "tenant"
    repo_id: "acme/payments"
    rank: 24
    alpha: 48
    target_modules: ["q_proj","k_proj","v_proj","o_proj","gate_proj","up_proj","down_proj"]
    acl: ["tenant_acme"]
  
  # Framework (Python/FastAPI)
  - id: "framework_fastapi_v1"
    hash: "b3:c9d0e1f2..."
    tier: "persistent"
    category: "framework"
    scope: "global"
    framework_id: "fastapi"
    framework_version: "0.104"
    rank: 12
    alpha: 24
    target_modules: ["q_proj","k_proj","v_proj","o_proj","gate_proj","up_proj","down_proj"]
  
  # Ephemeral (commit-scoped)
  - id: "commit_ab12cd34ef56"
    hash: "b3:g3h4i5j6..."
    tier: "ephemeral"
    category: "ephemeral"
    scope: "commit"
    repo_id: "acme/payments"
    commit_sha: "ab12cd34ef56"
    rank: 4
    alpha: 8
    target_modules: ["gate_proj","up_proj","down_proj"]
    ttl: 259200  # 72 hours
    acl: ["tenant_acme"]
    metadata:
      pr_id: "123"
      branch: "fix/payment-timeout"
      changed_files: ["src/payments/processor.py", "tests/test_processor.py"]

router:
  k_sparse: 3
  gate_quant: "q15"
  entropy_floor: 0.02
  tau: 1.0
  sample_tokens_full: 128
  features:
    - "lang_one_hot"
    - "framework_prior"
    - "symbol_hits"
    - "path_tokens"
    - "attn_entropy"
    - "commit_hint"
  code_features:
    enable_lang_detection: true
    enable_framework_prior: true
    enable_symbol_hits: true
    enable_path_tokens: true
    enable_commit_hint: true
    max_framework_adapters: 1

telemetry:
  schema_hash: "b3:q7r8s9t0..."
  sampling:
    token: 0.05
    router: 1.0
    inference: 1.0
  router_full_tokens: 128
  bundle:
    max_events: 500000
    max_bytes: 268435456

policies:
  egress:
    mode: "deny_all"
  access:
    adapters: "RBAC"
    datasets: "ABAC"
  evidence:
    require_open_book: true
    min_spans: 1
  code:
    evidence_min_spans: 1
    allow_auto_apply: true
    require_test_coverage: 0.8
    path_allowlist:
      - "src/**"
      - "lib/**"
      - "tests/**"
    path_denylist:
      - "**/.env"
      - "**/secrets/**"
      - "**/*.pem"
      - "**/*.key"
    allow_external_deps: false
    secret_patterns:
      - "(?i)(api[_-]?key|password|secret|token|aws[_-]?access)\\s*=\\s*['\"][^'\"]+['\"]"
    max_patch_size_lines: 500

seeds:
  global: "b3:u1v2w3x4..."
```

### Example 3: Multi-Framework

```yaml
schema: "adapteros.manifest.v4"

base:
  model_id: "Qwen2.5-7B-Instruct"
  model_hash: "b3:a1b2c3d4..."
  arch: "qwen2"
  vocab_size: 152064
  hidden_dim: 3584
  n_layers: 28
  n_heads: 28
  config_hash: "b3:..."
  tokenizer_hash: "b3:..."
  tokenizer_cfg_hash: "b3:..."

adapters:
  - id: "code_lang_v1"
    hash: "b3:..."
    tier: "persistent"
    category: "code"
    scope: "global"
    rank: 16
    alpha: 32
    target_modules: ["q_proj","k_proj","v_proj","o_proj","gate_proj","up_proj","down_proj"]
  
  # Backend frameworks
  - id: "framework_django_v1"
    hash: "b3:..."
    tier: "persistent"
    category: "framework"
    scope: "global"
    framework_id: "django"
    framework_version: "4.2"
    rank: 12
    alpha: 24
    target_modules: ["q_proj","k_proj","v_proj","o_proj","gate_proj","up_proj","down_proj"]
  
  - id: "framework_pytest_v1"
    hash: "b3:..."
    tier: "persistent"
    category: "framework"
    scope: "global"
    framework_id: "pytest"
    framework_version: "7.4"
    rank: 8
    alpha: 16
    target_modules: ["q_proj","v_proj","gate_proj","up_proj"]
    intent: "testing"
  
  # Frontend frameworks
  - id: "framework_react_v2"
    hash: "b3:..."
    tier: "persistent"
    category: "framework"
    scope: "global"
    framework_id: "react"
    framework_version: "18"
    rank: 12
    alpha: 24
    target_modules: ["q_proj","k_proj","v_proj","o_proj","gate_proj","up_proj","down_proj"]
  
  - id: "framework_nextjs_v1"
    hash: "b3:..."
    tier: "persistent"
    category: "framework"
    scope: "global"
    framework_id: "nextjs"
    framework_version: "14"
    rank: 12
    alpha: 24
    target_modules: ["q_proj","k_proj","v_proj","o_proj","gate_proj","up_proj","down_proj"]
  
  # Infrastructure
  - id: "framework_kubernetes_v1"
    hash: "b3:..."
    tier: "persistent"
    category: "framework"
    scope: "global"
    framework_id: "kubernetes"
    framework_version: "1.28"
    rank: 12
    alpha: 24
    target_modules: ["q_proj","k_proj","v_proj","o_proj","gate_proj","up_proj","down_proj"]
  
  # Codebase
  - id: "codebase_fullstack_app_v12"
    hash: "b3:..."
    tier: "persistent"
    category: "codebase"
    scope: "tenant"
    repo_id: "myorg/fullstack-app"
    rank: 28
    alpha: 56
    target_modules: ["q_proj","k_proj","v_proj","o_proj","gate_proj","up_proj","down_proj"]
    acl: ["tenant_myorg"]

router:
  k_sparse: 3
  gate_quant: "q15"
  entropy_floor: 0.02
  tau: 1.0
  sample_tokens_full: 128
  features:
    - "lang_one_hot"
    - "framework_prior"
    - "symbol_hits"
    - "path_tokens"
    - "attn_entropy"
    - "prompt_verb"
  code_features:
    enable_lang_detection: true
    enable_framework_prior: true
    enable_symbol_hits: true
    enable_path_tokens: true
    enable_commit_hint: false
    max_framework_adapters: 1  # Only one framework adapter per request

telemetry:
  schema_hash: "b3:q7r8s9t0..."
  sampling:
    token: 0.05
    router: 1.0
    inference: 1.0
  router_full_tokens: 128
  bundle:
    max_events: 500000
    max_bytes: 268435456

policies:
  egress:
    mode: "deny_all"
  access:
    adapters: "RBAC"
    datasets: "ABAC"
  evidence:
    require_open_book: true
    min_spans: 1
  code:
    evidence_min_spans: 1
    allow_auto_apply: false
    path_allowlist:
      - "backend/**"
      - "frontend/**"
      - "infrastructure/**"
      - "tests/**"
    path_denylist:
      - "**/.env*"
      - "**/secrets/**"
      - "**/*.pem"
      - "**/*.key"
      - "**/node_modules/**"
      - "**/__pycache__/**"
    allow_external_deps: false
    secret_patterns:
      - "(?i)(api[_-]?key|password|secret|token)\\s*[:=]\\s*['\"][^'\"]{8,}['\"]"
    max_patch_size_lines: 1000

seeds:
  global: "b3:u1v2w3x4..."
```

## Migration from V3

Existing V3 manifests remain valid. To enable code intelligence:

1. Update `schema` to `"adapteros.manifest.v4"`
2. Add `category` and `scope` to adapters
3. Add `router.features` and `router.code_features`
4. Add `policies.code` section

V3 manifests without code extensions will work but won't use code-specific features.

## Validation Rules

1. If `category == "framework"`, `framework_id` must be present
2. If `category == "codebase"`, `repo_id` must be present
3. If `category == "ephemeral"`, `commit_sha` must be present and `tier == "ephemeral"`
4. If `tier == "ephemeral"`, `ttl` must be present
5. `router.code_features.max_framework_adapters` must be ≥ 1
6. `policies.code.path_allowlist` and `path_denylist` must not overlap

## Backward Compatibility

- V3 readers ignore unknown fields
- V4 readers support V3 manifests (category defaults to inferred from tier)
- Deterministic hashing excludes unknown fields in V3 mode
