# Diagnostics Examples

This directory contains runnable examples for diagnostics workflows.

## Enable Diagnostics in Config

Add to your `configs/cp.toml` (or equivalent):

```toml
[diag]
enabled = true
level = "tokens"
channel_capacity = 1000
max_events_per_run = 10000
batch_size = 100
batch_timeout_ms = 500
```

## Run Local Diagnostics

```bash
aosctl diag run --full
aosctl diag run --system --json
aosctl diag run --tenant default --bundle ./diag_bundle.zip
```

## Export and Verify Bundles (Server API)

```bash
# Export a signed bundle for a specific trace ID
aosctl diag export --trace-id trace-abc123 -o bundle.tar.zst

# Include evidence payloads (requires a token)
aosctl diag export \
  --trace-id trace-abc123 \
  -o bundle.tar.zst \
  --include-evidence \
  --evidence-token $EVIDENCE_TOKEN

# Verify offline
aosctl diag verify bundle.tar.zst --verbose
```
