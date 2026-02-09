# Inference Verification

This document describes how to verify inference correctness and determinism in AdapterOS.

## Overview

AdapterOS provides multiple layers of verification to ensure inference correctness:

1. **Smoke Tests** - Quick health checks that verify basic inference functionality
2. **Golden Determinism Tests** - Tests that verify identical inputs produce identical outputs
3. **UI Self-Test** - Dashboard button for manual verification
4. **Run Detail Inspection** - View backend and thinking mode configuration for any run

## Smoke Tests

The smoke test runner (`aos-smoke`) provides quick verification of the inference pipeline.

### Location

```
tools/smoke/
├── Cargo.toml
└── src/main.rs
```

### Running

```bash
# Build the smoke test tool
cd tools/smoke
cargo build --release

# Run against local server
./target/release/aos-smoke

# Run against specific server
./target/release/aos-smoke --url http://localhost:8080

# Run with verbose output
./target/release/aos-smoke --verbose

# Run with receipt verification
./target/release/aos-smoke --verify-receipts
```

### Test Cases

The smoke test runner executes the following tests:

| Test | Description | Endpoint |
|------|-------------|----------|
| Health Check | Verify `/healthz` returns 200 | `/healthz` |
| Readiness Check | Verify `/readyz` returns ready state | `/readyz` |
| Non-Stream Inference | Simple inference without thinking mode | `/v1/infer` |
| Non-Stream Thinking | Inference with thinking/reasoning enabled | `/v1/infer` |
| Stream Inference | Streaming inference without thinking mode | `/v1/infer/stream` |
| Stream Thinking | Streaming inference with thinking mode | `/v1/infer/stream` |

### Exit Codes

- `0` - All tests passed
- `1` - One or more tests failed

### Output Format

```
╔══════════════════════════════════════════════════════════════╗
║           AdapterOS Inference Smoke Test                    ║
╠══════════════════════════════════════════════════════════════╣
║  Server: http://localhost:8080                               ║
║  Verify Receipts: yes                                        ║
╚══════════════════════════════════════════════════════════════╝

[1/6] Health check...                              [PASS]  12ms
[2/6] Readiness check...                           [PASS]  8ms
[3/6] Non-stream inference (thinking=false)...     [PASS]  523ms
      Trace: abc123...
      Receipt verified: yes
[4/6] Non-stream inference (thinking=true)...      [PASS]  1204ms
      Trace: def456...
      Receipt verified: yes
...

═══════════════════════════════════════════════════════════════
  Results: 6/6 passed
═══════════════════════════════════════════════════════════════
```

## Golden Determinism Tests

Golden determinism tests verify that identical inference requests produce identical outputs and receipts.

### Location

```
tests/
├── golden_determinism_test.rs  # Test harness
└── golden_runs.json            # Test cases
```

### Running

```bash
# Run golden tests (requires running server)
AOS_TEST_URL=http://localhost:8080 cargo test --test golden_determinism_test -- --ignored

# Or use default URL
export AOS_TEST_URL=http://localhost:8080
cargo test --test golden_determinism_test -- --ignored
```

### Test Cases

Each golden test case includes:

| Field | Description |
|-------|-------------|
| `id` | Unique test identifier |
| `description` | Human-readable description |
| `prompt` | The prompt to send |
| `seed` | Deterministic seed for reproducibility |
| `max_tokens` | Maximum tokens to generate |
| `temperature` | Sampling temperature (0.0 for deterministic) |
| `reasoning_mode` | Whether to enable thinking mode |
| `expected_contains` | Optional text that must appear in output |

Example test case:
```json
{
  "id": "simple_math",
  "description": "Simple arithmetic question with deterministic seed",
  "prompt": "What is 2+2? Answer with just the number.",
  "seed": 12345,
  "max_tokens": 10,
  "temperature": 0.0,
  "reasoning_mode": false,
  "expected_contains": "4"
}
```

### Verification Process

For each test case, the harness:

1. Runs the inference request twice
2. Compares output text and tokens
3. Compares receipt digests
4. Compares output digests
5. Optionally checks for expected content

A test passes if all comparisons match.

## UI Self-Test

The Dashboard includes a "Run Self-Test" button in Quick Actions that performs an immediate verification.

### How It Works

1. Click "Run Self-Test" in Dashboard Quick Actions
2. The UI sends a simple math prompt (`2+2`) with temperature 0.0
3. Results are displayed inline:
   - **Pass** (green): Response contains expected answer
   - **Fail** (red): Unexpected response or error

### Results Display

On success:
- Pass/fail status with checkmark
- Latency badge (e.g., "123ms")
- Backend badge (e.g., "mlx", "coreml")
- Links to View Run and Receipt tab

On failure:
- Error message explaining the failure
- Retry option

### Use Cases

- Quick verification after deployment
- Verify inference pipeline is working
- Generate a test run for inspection

## Run Detail Configuration

The Run Detail Overview tab shows execution configuration:

| Field | Description | Source |
|-------|-------------|--------|
| Stack | Adapter stack used | Diagnostic events (planned) |
| Model | Base model ID | Diagnostic events (planned) |
| Policy | Policy pack applied | Diagnostic events (planned) |
| Backend | Execution backend (mlx, coreml, metal) | InferenceTraceDetail.backend_id |
| Thinking Mode | Whether reasoning was enabled | Diagnostic event payload |

### Backend Values

| Value | Description |
|-------|-------------|
| `mlx` | MLX C++ backend (primary) |
| `coreml` | CoreML ANE acceleration |
| `metal` | Metal GPU kernels |

### Thinking Mode Values

| Value | Description |
|-------|-------------|
| Enabled | Request had `reasoning_mode: true` |
| Disabled | Request had `reasoning_mode: false` |
| Unknown | Reasoning mode not captured in events |

## Troubleshooting

### Smoke Tests Fail

1. **Health/Readiness fails**: Server not running or wrong URL
   ```bash
   # Check server is running
   curl http://localhost:8080/healthz
   ```

2. **Inference fails**: Model not loaded or backend issue
   ```bash
   # Check system status
   curl http://localhost:8080/v1/system/status
   ```

3. **Receipt verification fails**: Server not generating receipts
   - Ensure `determinism.generate_receipts = true` in config

### Golden Tests Show Different Hashes

1. **Seed not being respected**: Check temperature is 0.0
2. **Model changed**: Different model weights produce different outputs
3. **Backend changed**: Different backends may have subtle differences
4. **Version mismatch**: API schema version changed

### Self-Test Shows "Unknown" Backend

The backend is fetched from the inference trace detail endpoint. If it shows "Unknown":

1. The inference completed before trace was recorded
2. The trace endpoint returned no backend_id
3. Check `/v1/traces/inference/{trace_id}` response

## Related Documentation

- [Determinism Rules](../determinism.md) - Seed derivation and determinism guarantees
- [Receipt Verification](../receipts.md) - Cryptographic receipt format
- [API Reference](../api/inference.md) - Inference endpoint documentation
