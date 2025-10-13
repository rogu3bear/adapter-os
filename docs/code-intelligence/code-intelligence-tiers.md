# Code Intelligence Tier System

## Overview

The code intelligence stack introduces a **semantic five-tier hierarchy** for adapter organization while maintaining backward compatibility with the existing `persistent`/`ephemeral` lifecycle model.

## Tier Definitions

### Layer 1: Base

**Purpose**: General language model with no domain-specific training.

**Characteristics**:
- Model: Qwen2.5-7B-Instruct, Llama, etc.
- Quantization: int4 for efficiency
- Size: ~4-5 GB
- No LoRA deltas

**Role in code tasks**: Provides foundational language understanding, reasoning, and instruction following.

**Example**:
```yaml
base:
  model_id: "Qwen2.5-7B-Instruct"
  model_hash: "b3:..."
  arch: "qwen2"
  vocab_size: 152064
  hidden_dim: 3584
  n_layers: 28
  n_heads: 28
```

---

### Layer 2: Code (Domain Adapter)

**Purpose**: Generic coding knowledge across languages, patterns, and tooling.

**Characteristics**:
- Category: `code`
- Scope: `global`
- Tier (lifecycle): `persistent`
- Rank: 16
- Alpha: 32
- Targets: All 7 linear layers (q/k/v/o + gate/up/down)
- Size: ~75-95 MB (fp16)

**Training data**:
- Language syntax and semantics (Python, Rust, TypeScript, Go, Java)
- Static analysis reasoning (type errors, linter violations)
- Refactoring patterns (extract function, inline, rename)
- Test scaffolding
- Docstring and comment hygiene
- Secure coding patterns

**Evaluation gates**:
- Compile-pass delta ≥ target on synthetic repos
- Linter error reduction ≥ target
- No security anti-patterns introduced

**Example**:
```yaml
adapters:
  - id: "code_lang_v1"
    hash: "b3:..."
    tier: "persistent"
    category: "code"
    scope: "global"
    rank: 16
    alpha: 32
    target_modules: ["q_proj","k_proj","v_proj","o_proj","gate_proj","up_proj","down_proj"]
```

---

### Layer 3: Frameworks (Type Adapters)

**Purpose**: Stack-specific APIs, idioms, and conventions.

**Characteristics**:
- Category: `framework`
- Scope: `global` (IT-controlled) or `tenant` (custom frameworks)
- Tier (lifecycle): `persistent`
- Rank: 8-16 (depending on framework coverage)
- Alpha: 16-32
- Targets: All 7 linear layers
- Size: ~55-95 MB each

**Training data per framework**:
- Official documentation (offline snapshots)
- Cookbook tasks and scaffolds
- Common patterns (middleware, routing, forms, state management)
- Gotchas and anti-patterns (migrations, CSRF, async traps)

**Supported frameworks** (initial set):
- **Python**: `django`, `fastapi`, `flask`, `pytest`
- **JavaScript/TypeScript**: `react`, `nextjs`, `express`, `vue`
- **Rust**: `axum`, `actix-web`, `tokio`
- **Go**: `gin`, `chi`, `gorm`
- **Java**: `spring`, `hibernate`
- **Infrastructure**: `kubernetes`, `terraform`

**Routing rule**: Maximum **one framework adapter** per request to keep K manageable.

**Example**:
```yaml
adapters:
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
```

---

### Layer 4: Codebase (Domain Adapter, Tenant-Specific)

**Purpose**: Repository-specific knowledge: internal APIs, conventions, architecture decisions, house style.

**Characteristics**:
- Category: `codebase`
- Scope: `tenant` or `repo`
- Tier (lifecycle): `persistent`
- Rank: 16-32 (higher for large repos)
- Alpha: 32-64
- Targets: All 7 linear layers
- Size: ~110-190 MB

**Training data**:
- Repository READMEs and architecture docs
- ADRs (Architecture Decision Records)
- Internal library usage patterns
- Service contracts and RPC definitions
- Code comments and docstrings
- Closed PR descriptions vs merged diffs (auto-mined Q/A)
- Coding standards and style guides

**Purpose**: Prevents hallucinating function names, respects module boundaries, follows house conventions.

**Evaluation gates**:
- Style adherence score ≥ target
- Internal API usage correctness
- Convention violations per KLOC decreases

**Example**:
```yaml
adapters:
  - id: "codebase_acme_payments_v7"
    hash: "b3:..."
    tier: "persistent"
    category: "codebase"
    scope: "tenant"
    repo_id: "acme/payments"
    rank: 24
    alpha: 48
    target_modules: ["q_proj","k_proj","v_proj","o_proj","gate_proj","up_proj","down_proj"]
    acl: ["tenant_acme"]
```

---

### Layer 5: Ephemeral (Per-Commit, TTL-Bound)

**Purpose**: Short-lived adapter or soft-prompt tied to a specific commit or PR.

**Characteristics**:
- Category: `ephemeral`
- Scope: `commit` or `pr`
- Tier (lifecycle): `ephemeral`
- Rank: 4-8 (ultra-light)
- Alpha: 8-16
- Targets: Subset (typically `gate_proj`, `up_proj`, `down_proj`)
- Size: ~10-30 MB
- TTL: 24-72 hours (configurable)

**Two modes**:

#### Mode A: Zero-Train (Commit Delta Pack)
- No training required
- Generate **CDP (Commit Delta Pack)**:
  - `git diff`
  - Changed symbols and files
  - Failing tests and logs
  - Linter/type checker errors
  - Build logs
  - Ticket/PR text
- Ephemeral **router priors** bias toward relevant adapters
- Per-commit **mini index** of changed files + neighbors

#### Mode B: Micro-LoRA
- Train rank 4-8 LoRA on 20-200 synthetic pairs:
  - "Given failure X/log Y, produce fix for file Z"
  - Supervision from failing tests + diffs
- TTL set to PR lifetime
- Encrypted at rest
- **Never crosses tenants**

**Lifecycle**:
1. Created on commit/push
2. Auto-attached to worker for that repo+branch
3. Evicted on merge, close, or TTL expiry
4. Optionally distilled into codebase adapter (after review)

**Evaluation gates**:
- Targeted test pass improvement
- Strict TTL enforcement (logged)
- Zero cross-tenant leakage

**Example**:
```yaml
adapters:
  - id: "commit_ab12cd34"
    hash: "b3:..."
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
```

---

## Hybrid Tier Model

To preserve backward compatibility, we **extend** rather than replace the existing schema:

### Existing Fields (Preserved)
- `tier`: `"persistent"` | `"ephemeral"` (storage/lifecycle classification)

### New Fields (Additive)
- `category`: Semantic role (`"base"` | `"code"` | `"framework"` | `"codebase"` | `"ephemeral"`)
- `scope`: Boundary (`"global"` | `"tenant"` | `"repo"` | `"commit"`)
- `framework_id`: Optional framework identifier (e.g., `"django"`, `"react"`)
- `framework_version`: Optional version string
- `repo_id`: Optional repository identifier
- `commit_sha`: Optional commit hash (for ephemeral)
- `intent`: Optional task focus (e.g., `"testing"`, `"security"`, `"refactor"`)

### Mapping

| Category    | Typical Tier (lifecycle) | Scope    | Example ID                  |
|-------------|--------------------------|----------|-----------------------------|
| `base`      | N/A (no adapter)         | `global` | N/A                         |
| `code`      | `persistent`             | `global` | `code_lang_v1`              |
| `framework` | `persistent`             | `global` | `framework_django_v1`       |
| `codebase`  | `persistent`             | `tenant` | `codebase_myrepo_v3`        |
| `ephemeral` | `ephemeral`              | `commit` | `commit_abc123`             |

The router uses `category` and `scope` for semantic routing, while the registry uses `tier` for lifecycle management (eviction, TTL).

---

## Routing Strategy (K=3)

**Typical selections**:

1. **Code explanation task**:
   - `[codebase, code, framework_X]`

2. **Refactor with tests**:
   - `[codebase, code, ephemeral]` (if commit context exists)

3. **Framework scaffolding**:
   - `[framework_X, codebase, code]`

4. **Bug fix on PR**:
   - `[ephemeral, codebase, framework_X]` or `[ephemeral, codebase, code]`

**Constraints**:
- K capped at configured maximum (default 3)
- Entropy floor (0.02) prevents single-adapter monopoly
- Max one framework adapter per request
- Deterministic tie-breaking: `(score desc, adapter_id asc)`

---

## Memory Management

**Eviction order under pressure**:
1. Drop ephemeral adapters (TTL-based, cold first)
2. Reduce K by 1 (logged)
3. Evict cold framework adapters (LRU)
4. Evict warm framework adapters (usage-based)
5. Keep code + codebase resident if possible

**Headroom target**: ≥15% unified memory free at all times.

---

## Promotion & Lifecycle

### Code Adapter
- Trained once, versioned (v1, v2, ...)
- Promoted via standard CP promotion flow
- Evaluated on cross-language corpus

### Framework Adapters
- IT-controlled, signed bundles
- Tenants opt-in via manifest
- Versioned per framework release (e.g., `django_4_2_v1`)

### Codebase Adapters
- Per-tenant, per-repo
- Retrained periodically (monthly, quarterly)
- Versioned (v1, v2, v3, ...)
- Old versions retained for rollback

### Ephemeral Adapters
- Created automatically on commit
- TTL enforced strictly (24-72h default)
- Logged eviction
- Optionally promoted to codebase adapter after manual review

---

## Summary Table

| Tier       | Category    | Scope    | Rank  | Lifecycle   | Size      | Purpose                          |
|------------|-------------|----------|-------|-------------|-----------|----------------------------------|
| Base       | N/A         | `global` | N/A   | Permanent   | 4-5 GB    | General language model           |
| Code       | `code`      | `global` | 16    | Persistent  | ~80 MB    | Generic coding knowledge         |
| Framework  | `framework` | `global` | 8-16  | Persistent  | ~60-90 MB | Stack-specific APIs & idioms     |
| Codebase   | `codebase`  | `tenant` | 16-32 | Persistent  | ~110-190 MB | Repo conventions & internal APIs |
| Ephemeral  | `ephemeral` | `commit` | 4-8   | TTL (24-72h)| ~10-30 MB | Commit-specific context          |

This design balances memory efficiency, routing flexibility, and semantic clarity while maintaining full backward compatibility with existing AdapterOS infrastructure.
