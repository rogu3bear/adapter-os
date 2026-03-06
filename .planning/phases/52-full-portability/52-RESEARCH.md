# Phase 52: Full Portability - Research

**Researched:** 2026-03-04
**Domain:** Build bootstrapping, path relocation, zero-touch configuration
**Confidence:** HIGH

## Summary

Phase 52 is an infrastructure-only phase: no new features, just removing friction so AdapterOS works from a clean clone on any Apple Silicon Mac. The codebase already has strong foundations -- `rebase_var_path` handles relative var/ paths, `EffectiveConfig` provides built-in defaults, `BackendKind::Auto` exists as default, and `aosctl config show` is implemented. The work is about closing gaps in the fresh-clone experience.

Three deliverables: (1) a `bootstrap.sh` script for dependency installation, (2) hardening path resolution so nothing assumes a specific working directory or absolute path, and (3) making `./start` self-sufficient on first run with auto-creation of var/ structure and actionable error messages for missing models.

**Primary recommendation:** Work from the outside in -- start with bootstrap.sh (no existing code), then harden path resolution in `adapteros-core` and `adapteros-config`, then update `./start` for the fresh-clone flow.

<user_constraints>
## User Constraints (from CONTEXT.md)

### Locked Decisions
- Single idempotent `./bootstrap.sh` script that checks/installs all dependencies
- Homebrew and rustup are **required prerequisites** -- bootstrap fails with clear message if either is missing
- Bootstrap handles everything else: MLX headers, WASM target, other brew deps
- `./bootstrap.sh --verify` flag optionally runs `cargo check --workspace` + `./scripts/ui-check.sh` after installing deps
- Without `--verify`, bootstrap only checks/installs deps and exits fast
- All runtime paths resolve **relative to project root** (current behavior, made robust)
- Project root detection: walk up from CWD looking for marker (Cargo.toml or `.adapteros-root`); `AOS_ROOT` env var overrides if set
- **Exception: shared model cache** at `~/.cache/adapteros/models/` to avoid duplicating large weight files across clones
- Model path lookup is **layered**: `var/models/` (project-local) -> `~/.cache/adapteros/models/` (shared cache) -> `AOS_MODEL_PATH` env var (explicit). First hit wins.
- All other runtime data (DB, sockets, logs, PIDs) stays project-local under `var/`
- `./start` **auto-creates** `var/` directory structure if missing, with a warning: "First run detected. Run ./bootstrap.sh --verify to ensure all deps are present."
- Missing models: **fail fast** with actionable message: "No model found. Run: aosctl model pull <name>" -- user chooses which model
- Missing DB: **auto-migrate with log** -- "Initializing database (N migrations)..." so user knows what's happening on first boot
- Keep current `adapteros-config` system -- just ensure all defaults are built-in so no config file is required to exist
- **Zero env vars required** for default operation -- every var has a sensible built-in default. Only AOS_DEV_NO_AUTH needed for dev mode.
- Model backend: **auto-detect** available backends and pick best one. AOS_MODEL_BACKEND overrides if set.
- Add `aosctl config show` command that prints all resolved config values with their source

### Claude's Discretion
- Exact marker file choice for project root detection
- Pre-flight check implementation (shell script vs Rust binary)
- Auto-detect backend priority ordering
- `aosctl config show` output format

### Deferred Ideas (OUT OF SCOPE)
None -- discussion stayed within phase scope
</user_constraints>

<phase_requirements>
## Phase Requirements

| ID | Description | Research Support |
|----|-------------|-----------------|
| PORT-52-01 | System builds and runs on a fresh Apple Silicon Mac with only documented prerequisites | bootstrap.sh script, pre-flight dependency checks in ./start, zero-env-var defaults |
| PORT-52-02 | Runtime paths are relocatable (no hardcoded absolute paths) | Project root detection via marker walk, rebase_var_path hardening, layered model cache |
| PORT-52-03 | Configuration works without environment-specific overrides for default operation | EffectiveConfig built-in defaults, auto-detect backend, ConfigLoader require_manifest=false |
</phase_requirements>

## Standard Stack

### Core (Existing -- No New Dependencies)
| Library | Version | Purpose | Why Standard |
|---------|---------|---------|--------------|
| `adapteros-core` | workspace | `rebase_var_path`, `resolve_var_dir`, `repo_root_from`, `defaults.rs` | Central path resolution and defaults |
| `adapteros-config` | workspace | `ConfigLoader`, `EffectiveConfig`, `path_resolver.rs` | Configuration precedence system |
| `adapteros-cli` | workspace | `commands/config.rs`, `commands/preflight.rs` | CLI surface for config show/preflight |
| `adapteros-storage` | workspace | `PlatformUtils::aos_user_cache_dir()` | `~/.cache/adapteros` resolution |
| `dirs` | (already dep) | XDG/macOS platform directories | Used by `adapteros-storage` already |

### Supporting (Shell)
| Tool | Purpose | When to Use |
|------|---------|-------------|
| `brew` | Dependency installation | bootstrap.sh for MLX headers, etc. |
| `rustup` | Rust toolchain management | bootstrap.sh for WASM target |
| `lsof` / `curl` | Runtime checks | Already used by ./start for preflight |

### Alternatives Considered
| Instead of | Could Use | Tradeoff |
|------------|-----------|----------|
| Shell bootstrap.sh | Rust binary bootstrapper | Shell is simpler -- no chicken-and-egg (need Rust to build Rust bootstrapper). Shell can check for Rust itself. |
| Marker file walk | `CARGO_MANIFEST_DIR` at compile time | `CARGO_MANIFEST_DIR` works for Rust binaries but not shell scripts. Marker walk is universal. |

## Architecture Patterns

### Existing Project Root Detection (path_utils.rs)
The codebase already has `repo_root_from()` in `adapteros-core/src/path_utils.rs`:
```rust
fn repo_root_from(start: &Path) -> Option<PathBuf> {
    for dir in start.ancestors() {
        if dir.join("Cargo.lock").exists() || dir.join(".git").exists() {
            return Some(dir.to_path_buf());
        }
    }
    None
}
```
This is private and used only by `runtime_base_dir()`. The phase should:
1. Add `.adapteros-root` as a marker (in addition to `Cargo.lock` and `.git`)
2. Respect `AOS_ROOT` env var as override
3. Make the function `pub` so shell scripts and other crates can query it

### Existing Config Default Chain
```
CLI args > ENV vars (AOS_*) > .env file > manifest TOML > compiled defaults
```
The `ConfigLoader` already handles `require_manifest: false` gracefully -- it logs a warning and proceeds with defaults. The `EffectiveConfig` sections all have `Default` impls. This chain is sound; the work is ensuring no config path *requires* a file to exist.

### Existing var/ Auto-Creation (start script)
```bash
ensure_var_dirs() {
    mkdir -p "$AOS_VAR_DIR/run"
    mkdir -p "$AOS_LOG_DIR"
    ensure_backend_log_alias
}
```
Called 12+ times throughout `./start`. Already handles creating var/run and var/logs. Needs extension for var/models, var/adapters, var/datasets, var/artifacts, etc.

### Layered Model Path Resolution (New)
```
1. var/models/<model-id>       (project-local)
2. ~/.cache/adapteros/models/  (shared cross-project cache)
3. AOS_MODEL_PATH env var      (explicit override)
```
The shell script `model-config.sh` already does similar resolution via `aos_resolve_model_runtime_env()`. The Rust side in `path_resolver.rs` resolves via `resolve_model_path()` but doesn't check `~/.cache/adapteros/models/`. The `PlatformUtils::aos_user_cache_dir()` already returns `~/.cache/adapteros`.

### Recommended Task Structure
```
bootstrap.sh                    # New file, top-level
crates/adapteros-core/
  src/path_utils.rs             # Harden repo_root_from, add AOS_ROOT, make pub
  src/defaults.rs               # Fix DEV_MODEL_PATH (currently /var/models/ absolute)
crates/adapteros-config/
  src/path_resolver.rs          # Add ~/.cache/adapteros/models/ layer
  src/model.rs                  # Update model resolution for layered lookup
  src/effective.rs              # Ensure all sections have sensible defaults without config file
  src/loader.rs                 # Verify require_manifest=false works end-to-end
start                           # Add first-run detection, expand ensure_var_dirs, model-missing message
scripts/lib/model-config.sh     # Add shared cache lookup layer
```

### Anti-Patterns to Avoid
- **Absolute paths in compiled constants:** `DEV_MODEL_PATH = "/var/models/Qwen3.5-27B"` uses absolute `/var/` (system directory, not project `var/`). This is a portability bug.
- **Requiring .env for defaults:** The `.env` file should be optional. All defaults must be compiled in.
- **Testing with hardcoded user paths:** Never use `/Users/star/...` in test assertions.
- **Creating var/ subdirs on import:** Create them lazily when needed, not eagerly on every startup.

## Don't Hand-Roll

| Problem | Don't Build | Use Instead | Why |
|---------|-------------|-------------|-----|
| XDG cache dir resolution | Manual `$HOME/.cache/` string concat | `dirs::cache_dir()` or existing `PlatformUtils::aos_user_cache_dir()` | Handles platform differences, XDG_CACHE_HOME override |
| Homebrew package check | Parse `brew list` output | `brew ls --versions <pkg>` (exit code) | Idempotent, handles edge cases |
| WASM target check | Parse rustup output | `rustup target list --installed \| grep wasm32` | Standard rustup interface |
| Shell argument parsing | Manual getopts in bootstrap.sh | Simple case/shift pattern | bootstrap.sh has only --verify, no complex args needed |

## Common Pitfalls

### Pitfall 1: DEV_MODEL_PATH is /var/models/ (system path)
**What goes wrong:** `DEV_MODEL_PATH` in `defaults.rs` is `/var/models/Qwen3.5-27B` -- an absolute path pointing to the *system* `/var`, not the project-relative `var/`. On a fresh clone this resolves to a nonexistent system directory.
**Why it happens:** Historical artifact from when models were stored system-wide.
**How to avoid:** Change to `var/models/Qwen3.5-27B` (relative, like all other defaults). The `rebase_var_path` function will absolutize it correctly.
**Warning signs:** ModelConfig tests fail when running from a non-project directory.

### Pitfall 2: Config file assumed to exist
**What goes wrong:** `require_manifest: true` is the `LoaderOptions` default. If `configs/cp.toml` is referenced but missing (e.g., user deleted it), config loading fails.
**Why it happens:** Default LoaderOptions assumes a manifest exists.
**How to avoid:** The boot path should use `require_manifest: false` when loading config, falling back to compiled defaults. The `./start` script currently passes `AOS_CONFIG_PATH` pointing at `configs/cp.toml` -- this works because that file ships with the repo, but the Rust loader should handle its absence gracefully.
**Warning signs:** "Config file not found" errors on fresh clone with minimal checkout.

### Pitfall 3: repo_root_from() fails outside git repo
**What goes wrong:** `repo_root_from()` checks for `Cargo.lock` and `.git`. If neither exists (e.g., downloaded tarball without git init), it returns `None` and `runtime_base_dir()` falls back to CWD.
**Why it happens:** Tarballs don't include `.git/`.
**How to avoid:** Add `.adapteros-root` as a marker file (committed to repo). Create it as an empty file at project root.
**Warning signs:** `resolve_var_dir()` creates `var/` in CWD instead of project root when running from a subdirectory.

### Pitfall 4: bootstrap.sh Homebrew race conditions
**What goes wrong:** Multiple `brew install` calls running concurrently can fail with lock contention.
**Why it happens:** brew uses a global lock file.
**How to avoid:** Run brew installs sequentially in bootstrap.sh. Check before install: `brew ls --versions <pkg> || brew install <pkg>`.
**Warning signs:** Intermittent "Another active Homebrew process" errors.

### Pitfall 5: Model path resolution order conflicts
**What goes wrong:** The CONTEXT.md specifies `var/models/` -> `~/.cache/adapteros/models/` -> `AOS_MODEL_PATH`, but the *existing* code in `model-config.sh` treats `AOS_MODEL_PATH` as *first* priority (env override). If we make `AOS_MODEL_PATH` last, it breaks the precedence principle.
**Why it happens:** The CONTEXT.md describes "first hit wins" for *discovery* (where does the model live?), not for *override* (what did the user explicitly request?). `AOS_MODEL_PATH` is an explicit user override and should always take priority.
**How to avoid:** Clarify: when `AOS_MODEL_PATH` is set, it's the *only* path checked (explicit override). When *not* set, search `var/models/<id>` then `~/.cache/adapteros/models/<id>`. This matches the existing precedence of ENV > defaults.
**Warning signs:** User sets `AOS_MODEL_PATH` but system ignores it.

### Pitfall 6: ensure_var_dirs creates too many directories eagerly
**What goes wrong:** Creating every possible `var/` subdirectory on startup adds latency and creates directories that may never be used.
**Why it happens:** Defensive programming.
**How to avoid:** Only create `var/`, `var/run`, and `var/logs` on startup (the minimum for boot). Other subdirectories (`var/adapters`, `var/datasets`, `var/artifacts`) should be created lazily by the code that needs them (e.g., `AdapterPaths::ensure_exists()`).
**Warning signs:** Excessive `mkdir` calls in startup logs.

## Code Examples

### Pattern 1: Idempotent Dependency Check in bootstrap.sh
```bash
# Source: Standard Homebrew CLI patterns
check_brew_pkg() {
    local pkg="$1"
    if brew ls --versions "$pkg" >/dev/null 2>&1; then
        echo "  [ok] $pkg"
        return 0
    fi
    echo "  [installing] $pkg..."
    brew install "$pkg"
}
```

### Pattern 2: Project Root Detection with AOS_ROOT Override
```rust
// Source: Existing adapteros-core/src/path_utils.rs, extended
const ROOT_MARKERS: &[&str] = &[".adapteros-root", "Cargo.lock", ".git"];

pub fn find_project_root(start: &Path) -> Option<PathBuf> {
    // ENV override takes absolute priority
    if let Ok(root) = std::env::var("AOS_ROOT") {
        let p = PathBuf::from(&root);
        if p.is_absolute() && p.exists() {
            return Some(p);
        }
    }
    // Walk up from start looking for markers
    for dir in start.ancestors() {
        for marker in ROOT_MARKERS {
            if dir.join(marker).exists() {
                return Some(dir.to_path_buf());
            }
        }
    }
    None
}
```

### Pattern 3: Layered Model Discovery
```rust
// Source: Extending adapteros-config/src/path_resolver.rs
pub fn discover_model_path(model_id: &str) -> Option<PathBuf> {
    // 1. Project-local var/models/
    let local = rebase_var_path("var/models").join(model_id);
    if local.exists() {
        return Some(local);
    }
    // 2. Shared user cache ~/.cache/adapteros/models/
    if let Ok(cache_dir) = adapteros_storage::platform::PlatformUtils::aos_user_cache_dir() {
        let shared = cache_dir.join("models").join(model_id);
        if shared.exists() {
            return Some(shared);
        }
    }
    None
}
```

### Pattern 4: First-Run Detection in start Script
```bash
# Source: Extending existing ensure_var_dirs pattern
first_run_check() {
    if [ ! -d "$AOS_VAR_DIR" ]; then
        echo ""
        echo -e "${FG_YELLOW}First run detected.${FG_RESET}"
        echo "  Run ./bootstrap.sh --verify to ensure all dependencies are installed."
        echo ""
    fi
}
```

### Pattern 5: Pre-flight Dependency Check
```bash
# Source: Extending check_start_dependencies in start script
check_build_deps() {
    local failed=0
    # MLX headers
    if ! brew ls --versions mlx >/dev/null 2>&1; then
        fg_error "Missing MLX headers: brew install ml-explore/mlx/mlx"
        failed=1
    fi
    # WASM target
    if ! rustup target list --installed 2>/dev/null | grep -q wasm32-unknown-unknown; then
        fg_error "Missing WASM target: rustup target add wasm32-unknown-unknown"
        failed=1
    fi
    return $failed
}
```

## State of the Art

| Old Approach | Current Approach | When Changed | Impact |
|--------------|------------------|--------------|--------|
| `DEV_MODEL_PATH = "/var/models/..."` (absolute) | Should be `"var/models/..."` (relative) | Phase 52 | Fixes portability for dev fixtures |
| Single model path (env or default) | Layered discovery (local -> cache -> env) | Phase 52 | Avoids duplicating 15GB+ model files across clones |
| Manual dependency setup | `bootstrap.sh` idempotent installer | Phase 52 | Fresh clone to running in < 5 minutes |
| Port-only preflight | Build-dependency preflight | Phase 52 | Fail fast before 10-minute cargo build |

## Validation Architecture

### Test Framework
| Property | Value |
|----------|-------|
| Framework | cargo test (built-in) |
| Config file | Cargo.toml (workspace) |
| Quick run command | `cargo test -p adapteros-config -p adapteros-core` |
| Full suite command | `cargo test --workspace` |
| Estimated runtime | ~45 seconds (quick), ~5 min (full) |

### Phase Requirements to Test Map
| Req ID | Behavior | Test Type | Automated Command | File Exists? |
|--------|----------|-----------|-------------------|-------------|
| PORT-52-01 | bootstrap.sh installs deps and verifies | integration/shell | `bash bootstrap.sh --verify` (manual on CI) | Wave 0 gap |
| PORT-52-01 | Pre-flight catches missing deps | unit | `cargo test -p adapteros-config --test config_validation_tests` | Partial |
| PORT-52-02 | No hardcoded absolute paths in defaults | unit | `cargo test -p adapteros-core -- defaults` | Wave 0 gap |
| PORT-52-02 | Project root detection with markers | unit | `cargo test -p adapteros-core -- path_utils` | Wave 0 gap |
| PORT-52-02 | Model discovery layered resolution | integration | `cargo test -p adapteros-config --test model_path_integration_test` | Exists, extend |
| PORT-52-03 | Config loads without manifest file | unit | `cargo test -p adapteros-config -- test_missing_config_allowed_when_not_required` | Exists |
| PORT-52-03 | All config sections have sensible defaults | unit | `cargo test -p adapteros-config -- effective` | Partial |

### Nyquist Sampling Rate
- **Minimum sample interval:** After every committed task -> run: `cargo test -p adapteros-config -p adapteros-core`
- **Full suite trigger:** Before merging final task of any plan wave
- **Phase-complete gate:** Full suite green before `/gsd:verify-work`
- **Estimated feedback latency per task:** ~20 seconds

### Wave 0 Gaps (must be created before implementation)
- [ ] `tests/bootstrap_smoke_test.sh` -- verifies bootstrap.sh is idempotent and checks deps
- [ ] Test in `adapteros-core/src/path_utils.rs` -- `find_project_root` with `.adapteros-root` marker
- [ ] Test in `adapteros-core/src/defaults.rs` -- assert no defaults use absolute paths (guard test)
- [ ] Test in `adapteros-config` -- model discovery with layered cache lookup
- [ ] `.adapteros-root` marker file -- empty file at project root (committed)

## Open Questions

1. **Model path precedence clarification**
   - What we know: CONTEXT.md says layered lookup `var/models/ -> ~/.cache/adapteros/models/ -> AOS_MODEL_PATH`. Existing code treats `AOS_MODEL_PATH` as highest-priority override.
   - What's unclear: Should `AOS_MODEL_PATH` be an override (first checked) or fallback (last checked)?
   - Recommendation: Treat `AOS_MODEL_PATH` as explicit override (checked first, matching existing precedence). When not set, discover via `var/models/` then `~/.cache/`. This is consistent with the CLI > ENV > default precedence throughout the config system.

2. **`aosctl config show` -- already exists**
   - What we know: `aosctl config show` already displays effective config with source attribution (table, env, and JSON formats). `aosctl config show-effective` also exists.
   - What's unclear: Whether the user wants enhancements or if existing implementation satisfies PORT-52-03.
   - Recommendation: Verify existing `aosctl config show` output covers all resolved values. If it does, this is already done. Only add source annotations if missing.

3. **DB auto-migration on first run**
   - What we know: `./start` seeds models but doesn't explicitly show migration progress. The server binary runs migrations on boot internally.
   - What's unclear: Whether the migration logging happens at the `./start` script level or inside the Rust server process.
   - Recommendation: The Rust server already runs migrations via sqlx on boot. Add a log line at INFO level: "Initializing database (applying N migrations)..." visible in `./start` output.

## Sources

### Primary (HIGH confidence)
- `crates/adapteros-core/src/path_utils.rs` -- project root detection, var path rebasing
- `crates/adapteros-core/src/defaults.rs` -- all compiled-in default constants
- `crates/adapteros-config/src/loader.rs` -- config loading with precedence
- `crates/adapteros-config/src/effective.rs` -- unified effective config with sections
- `crates/adapteros-config/src/path_resolver.rs` -- path resolution with fallbacks
- `crates/adapteros-config/src/model.rs` -- model config and backend preference
- `crates/adapteros-cli/src/commands/config.rs` -- existing `aosctl config show` implementation
- `crates/adapteros-storage/src/platform/common.rs` -- `PlatformUtils::aos_user_cache_dir()`
- `start` script (2292 lines) -- boot orchestration, var creation, preflight, model seeding
- `scripts/lib/model-config.sh` -- shell-side model path resolution

### Secondary (MEDIUM confidence)
- `configs/cp.toml` -- default config file shipped with repo
- `scripts/lib/freeze-guard.sh` -- port conflict detection

## Metadata

**Confidence breakdown:**
- Standard stack: HIGH -- all code is in the repo, no external libraries needed
- Architecture: HIGH -- extending existing patterns (rebase_var_path, ConfigLoader, ensure_var_dirs)
- Pitfalls: HIGH -- identified from direct code inspection of defaults.rs, path_utils.rs, loader.rs

**Research date:** 2026-03-04
**Valid until:** 2026-04-04 (stable infrastructure, no external API dependencies)
