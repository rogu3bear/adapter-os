% aosctl — AdapterOS CLI Manual

This manual provides an overview of the `aosctl` command‑line interface, including command groups, flag conventions, and usage examples. It is intended as a stable high‑level reference; for exhaustive per‑flag help, run `aosctl <command> --help`.

## 1. Global Flags and Conventions

- `--json` / `-q` / `--quiet`  
  - `--json`: machine‑readable JSON output when supported.  
  - `-q, --quiet`: suppress non‑essential human output (errors still printed).  
- `-v, --verbose`  
  - Enables verbose progress output for long‑running operations.  
- Exit codes  
  - `0` on success.  
  - Non‑zero on failure; error codes (e.g. `E2002`) can be inspected via `aosctl explain` and `aosctl error-codes`.
- Help and completion  
  - `aosctl --help` and `aosctl <command> --help` for detailed usage.  
  - `aosctl completions <shell>` to generate shell completion scripts.

### Output Modes

- Human‑friendly text (default): structured sections, progress lines, and tables.  
- JSON: structured JSON, suitable for automation (`--json`).  
- Quiet: minimal output for scripts/CI (`--quiet` or CI auto‑detection).

## 2. Tenant Management

Tenant commands create and manage isolated tenants on a node.

- `aosctl init-tenant` / `aosctl init`  
  - Initialize a new tenant with specific UID/GID.  
  - Key flags: `--id`, `--uid`, `--gid`.

**Examples**

- Create a development tenant:

```bash
aosctl init-tenant --id tenant_dev --uid 1000 --gid 1000
```

- Create a production tenant:

```bash
aosctl init-tenant --id tenant_prod --uid 5000 --gid 5000
```

## 3. Adapter Management

Adapter commands manage adapters in the registry (listing, registration, pinning, and air‑gap transfers).

- List adapters  
  - `aosctl list-adapters`  
  - Key flags: `--tier` (filter by tier), `--json` for machine‑readable output.
- Register adapters  
  - `aosctl register-adapter <id> <hash>`  
  - Key flags: `--tier`, `--rank`.
- Pin and unpin adapters  
  - `aosctl pin-adapter` / `aosctl unpin-adapter`  
  - Key flags: `--tenant`, `--adapter`, `--reason`, `--ttl-hours`.
- Air‑gap import/export  
  - `aosctl adapter export` / `aosctl adapter import` or equivalent adapter bundle commands.

**Examples**

- List adapters for scripting:

```bash
aosctl list-adapters --tier persistent --json > adapters.json
```

- Register a persistent adapter:

```bash
aosctl register-adapter my_adapter b3:abc123... \
  --tier persistent \
  --rank 16
```

## 4. Inference and Replay

These commands interact with running workers and telemetry bundles.

- `aosctl infer`  
  - Run an inference against a worker UDS.  
  - Key flags: `--adapter`, `--prompt`, `--socket`, `--max-tokens`, `--timeout`, `--require-evidence`.
- `aosctl replay`  
  - Replay a telemetry bundle and optionally check determinism.

**Examples**

- Basic inference:

```bash
aosctl infer --prompt "Hello world" \
  --socket /var/run/adapteros.sock
```

- Inference using a specific adapter:

```bash
aosctl infer --adapter my_adapter \
  --prompt "Use adapter" \
  --socket /var/run/adapteros.sock
```

## 5. Metrics and Diagnostics

These commands inspect system health, metrics, and diagnostic bundles.

- Metrics and health  
  - `aosctl metrics ...` subcommands (current metrics, policy thresholds, violations).  
  - Output mode is frequently chosen via `OutputMode::from_env` for CI‑friendly defaults.
- Diagnostic bundle  
  - `aosctl diag-bundle` (or similar) to collect logs and telemetry into an archive.
- SECD status  
  - `aosctl secd-status`  
  - Key flags: `--database`, `--json`.

**Examples**

- Show SECD status in JSON:

```bash
aosctl secd-status --json > secd-status.json
```

- Generate a diagnostic bundle:

```bash
aosctl diag-bundle --output ./var/diag-bundle.zip
```

## 6. Telemetry and Verification

These commands operate on telemetry bundles and CLI telemetry.

- Telemetry verification  
  - `aosctl telemetry-verify --bundle-dir ./var/telemetry`  
  - Validates bundles produced by `adapteros-telemetry` against policy.
- Error explanation and codes  
  - `aosctl explain <ERROR_CODE>`: explain a specific error code (e.g. `E2002`).  
  - `aosctl error-codes`: list canonical codes and documentation links.

**Examples**

- Verify telemetry bundles:

```bash
aosctl telemetry-verify --bundle-dir ./var/telemetry
```

- Explain an error code:

```bash
aosctl explain E2002
```

## 7. Codegraph and Callgraph

Codegraph‑related commands interact with the AdapterOS codegraph database.

- `aosctl callgraph-export`  
  - Export a callgraph from a codegraph database.  
  - Key flags: `--codegraph-db`, `--output`, `--format`.

**Example**

```bash
aosctl callgraph-export \
  --codegraph-db ./var/codegraph.db \
  --output graph.dot
```

## 8. Learning Resources

These commands provide documentation and tutorials directly in the CLI.

- `aosctl tutorial`  
  - Guided walkthrough of common workflows (tenant setup, adapter registration, inference).
- `aosctl manual`  
  - Prints this manual in the terminal.  
  - Use `--help` on specific commands for additional sections and examples.

## 9. Scripting and CI Recommendations

- Prefer `--json` when integrating with tools and pipelines.  
- Use `--quiet` (or rely on CI auto‑detection) for minimal noise; errors and structured telemetry events are still emitted.  
- When a command fails with an error code like `E2002`, capture the event ID from the message:

```text
✗ E2002 – see: aosctl explain E2002 (event: <EVENT_ID>)
```

You can then use `<EVENT_ID>` with future tooling to look up the corresponding telemetry event produced by the CLI.

[source: crates/adapteros-cli/src/main.rs L1-L220]
