# CLI Chat MVP - Implementation Plan

**Status**: Draft
**Created**: 2026-01-05
**Target**: CLI chat with loaded model (no server required)

---

## Executive Summary

This document defines the minimal implementation path for "CLI chat with loaded model" - enabling `aosctl chat` to run inference directly without requiring a running HTTP server or UDS worker.

---

## Current Architecture

### CLI Commands (adapteros-cli)

| Command | Mode | Backend | Requires Server |
|---------|------|---------|-----------------|
| `aosctl chat interactive` | HTTP | SSE to `:8080/api/v1/infer/stream` | Yes |
| `aosctl chat prompt` | HTTP | POST to `:8080/api/v1/infer` | Yes |
| `aosctl infer` | UDS | Unix socket to worker.sock | Yes (worker) |
| `aosctl serve` | UDS | Starts worker + UDS server | Creates server |

**Current limitation**: All inference paths require either HTTP server or UDS worker running.

### Model Runtime Layers

```
┌─────────────────────────────────────────────────────────┐
│                   HTTP API (8080)                       │
│                 handlers/inference.rs                   │
└──────────────────────────┬──────────────────────────────┘
                           │
┌──────────────────────────▼──────────────────────────────┐
│              InferenceCore (server-api)                 │
│                   Worker Pool                           │
└──────────────────────────┬──────────────────────────────┘
                           │
┌──────────────────────────▼──────────────────────────────┐
│                Worker (lora-worker)                     │
│   - manifest, tokenizer, telemetry, policy, RAG        │
│   - inference_pipeline.rs                              │
└──────────────────────────┬──────────────────────────────┘
                           │
┌──────────────────────────▼──────────────────────────────┐
│           TextGenerationKernel Trait                    │
│             (lora-kernel-api)                           │
│   - generate_text_full()                               │
│   - generate_text_stream()                             │
│   - generate_text_complete()                           │
└──────────────────────────┬──────────────────────────────┘
                           │
┌──────────────────────────▼──────────────────────────────┐
│                Backend Implementations                  │
│   - MLX FFI (adapteros-lora-mlx-ffi)                   │
│   - MLX Subprocess Bridge (Python mlx-lm)              │
│   - Metal (adapteros-lora-kernel-mtl)                  │
│   - CoreML (adapteros-lora-kernel-coreml)              │
└─────────────────────────────────────────────────────────┘
```

### Where "Model Runtime" Exists

1. **Full Worker** (`adapteros-lora-worker/src/lib.rs:1005`):
   - `Worker::new()` - Full initialization with manifest, telemetry, policy
   - `Worker::infer()` / `Worker::infer_stream()` - Production inference
   - Heavy dependencies: manifest, RAG, telemetry, quota manager

2. **TextGenerationKernel trait** (`adapteros-lora-kernel-api/src/lib.rs:1135`):
   - `generate_text_full()` - Non-streaming generation
   - `generate_text_stream()` - Streaming generation (iterator-based)
   - Lightweight interface for direct model access

3. **MLX FFI** (`adapteros-lora-mlx-ffi`):
   - Implements `TextGenerationKernel`
   - Direct C++ FFI to MLX library
   - Supports model loading, generation, tokenization

4. **MLX Subprocess Bridge** (`adapteros-lora-worker/src/mlx_subprocess_bridge.rs`):
   - JSON protocol to Python `mlx-lm` subprocess
   - Used for MoE models not supported by FFI
   - Has streaming support via callback/iterator patterns

---

## Recommended Minimal Path

### Option A: Direct MLX FFI (Recommended)

Use `adapteros-lora-mlx-ffi` directly to bypass the full Worker infrastructure:

```
aosctl chat --local --model-path ./var/models/Qwen2.5-7B-Instruct
```

**Why this approach:**
- MLX FFI already implements `TextGenerationKernel`
- Supports both `generate_text_full()` and `generate_text_stream()`
- Handles tokenization internally
- No manifest, telemetry, or policy overhead for MVP

### Option B: Subprocess Bridge

Fall back to Python `mlx-lm` via subprocess protocol for MoE models:

```
aosctl chat --local --model-path ./var/models/Qwen3-MoE-4bit --backend mlx-subprocess
```

**When to use:**
- MoE models (Mixture of Experts) not supported by FFI
- When FFI backend fails to initialize

---

## Implementation Plan

### Phase 1: Local Chat Mode

Add `--local` flag to `aosctl chat` that enables embedded inference:

**File: `crates/adapteros-cli/src/commands/chat.rs`**

```rust
pub struct ChatArgs {
    // Existing fields...

    /// Run in local mode (no server required)
    #[arg(long)]
    pub local: bool,

    /// Model path for local mode (e.g., ./var/models/Qwen2.5-7B-Instruct)
    #[arg(long, requires = "local")]
    pub model_path: Option<PathBuf>,

    /// Backend for local mode: mlx, mlx-subprocess
    #[arg(long, default_value = "mlx")]
    pub backend: LocalBackend,
}
```

### Phase 2: LocalInferenceEngine

Create a lightweight inference engine that wraps MLX directly:

**File: `crates/adapteros-cli/src/local_inference.rs` (new)**

```rust
use adapteros_lora_kernel_api::TextGenerationKernel;
use adapteros_lora_mlx_ffi::MlxBackend;

pub struct LocalInferenceEngine {
    backend: Box<dyn TextGenerationKernel>,
    model_path: PathBuf,
}

impl LocalInferenceEngine {
    pub fn new(model_path: &Path) -> Result<Self> {
        // 1. Initialize MLX runtime
        adapteros_lora_mlx_ffi::mlx_runtime_init()?;

        // 2. Load model via MLX FFI
        let backend = MlxBackend::from_path(model_path)?;

        Ok(Self {
            backend: Box::new(backend),
            model_path: model_path.to_path_buf(),
        })
    }

    pub fn generate(&self, prompt: &str, max_tokens: usize) -> Result<String> {
        let result = self.backend.generate_text_full(
            prompt,
            max_tokens,
            0.7,  // temperature
            0.95, // top_p
            &[], // stop sequences
        )?;
        Ok(result.text)
    }

    pub fn generate_stream<F>(&self, prompt: &str, max_tokens: usize, callback: F) -> Result<()>
    where
        F: FnMut(&str) -> bool,
    {
        self.backend.generate_text_stream(prompt, max_tokens, 0.7, 0.95, callback)?;
        Ok(())
    }
}
```

### Phase 3: REPL Integration

Update `interactive` subcommand to use local engine when `--local`:

```rust
async fn run_interactive(args: &ChatArgs) -> Result<()> {
    if args.local {
        run_interactive_local(args).await
    } else {
        run_interactive_server(args).await  // Existing HTTP-based code
    }
}

async fn run_interactive_local(args: &ChatArgs) -> Result<()> {
    let model_path = args.model_path.as_ref()
        .ok_or_else(|| anyhow!("--model-path required for local mode"))?;

    let engine = LocalInferenceEngine::new(model_path)?;
    let mut rl = rustyline::DefaultEditor::new()?;

    println!("Local chat mode (model: {})", model_path.display());
    println!("Type 'exit' or Ctrl-D to quit\n");

    loop {
        let line = match rl.readline("You: ") {
            Ok(line) => line,
            Err(ReadlineError::Eof) | Err(ReadlineError::Interrupted) => break,
            Err(e) => return Err(e.into()),
        };

        if line.trim() == "exit" {
            break;
        }

        print!("Assistant: ");
        std::io::stdout().flush()?;

        engine.generate_stream(&line, 512, |token| {
            print!("{}", token);
            std::io::stdout().flush().unwrap();
            true
        })?;

        println!("\n");
    }

    Ok(())
}
```

---

## Proposed CLI UX

### Commands

```bash
# Server mode (existing - unchanged)
aosctl chat interactive                    # REPL via HTTP server
aosctl chat prompt "Hello world"           # Single prompt via HTTP

# Local mode (new)
aosctl chat interactive --local --model-path ./var/models/Qwen2.5-7B-Instruct
aosctl chat prompt "Hello" --local --model-path ./var/models/Qwen2.5-7B-Instruct

# With options
aosctl chat interactive --local \
  --model-path ./var/models/Qwen2.5-7B-Instruct \
  --temperature 0.8 \
  --max-tokens 1024
```

### Flags

| Flag | Description | Default |
|------|-------------|---------|
| `--local` | Enable local mode (no server) | false |
| `--model-path <PATH>` | Path to model directory | Required in local mode |
| `--backend <mlx\|mlx-subprocess>` | Backend type | `mlx` |
| `--temperature <FLOAT>` | Sampling temperature | 0.7 |
| `--top-p <FLOAT>` | Nucleus sampling | 0.95 |
| `--max-tokens <N>` | Max tokens to generate | 512 |

### Environment Variables

| Variable | Description |
|----------|-------------|
| `AOS_MODEL_PATH` | Default model path for `--local` mode |
| `AOS_TOKENIZER_PATH` | Override tokenizer path |

### Example Session

```
$ aosctl chat interactive --local --model-path ./var/models/Qwen2.5-7B-Instruct

AdapterOS Chat (Local Mode)
Model: ./var/models/Qwen2.5-7B-Instruct
Backend: MLX FFI
Type 'exit' or Ctrl-D to quit

You: What is the capital of France?
Assistant: The capital of France is Paris.

You: Tell me more about it.
Assistant: Paris is the largest city in France and serves as the country's...

You: exit
Goodbye!
```

---

## Acceptance Criteria

### Must Have (MVP)

- [ ] `aosctl chat interactive --local` works without running server
- [ ] `aosctl chat prompt --local` works for single prompts
- [ ] Model loading via `--model-path` flag
- [ ] Streaming token output in REPL mode
- [ ] Clean error messages when model path is invalid
- [ ] Ctrl-C / Ctrl-D exits gracefully

### Should Have

- [ ] `--temperature` and `--top-p` flags work
- [ ] `--max-tokens` flag works
- [ ] `AOS_MODEL_PATH` environment variable fallback
- [ ] Progress indicator during model loading
- [ ] Memory usage reporting on exit

### Nice to Have

- [ ] `--backend mlx-subprocess` for MoE models
- [ ] Conversation history (multi-turn context)
- [ ] System prompt support (`--system`)
- [ ] JSON output mode (`--json`)

---

## Test Plan

### Unit Tests

```rust
// crates/adapteros-cli/src/local_inference.rs

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_model_path_validation() {
        let result = LocalInferenceEngine::new(Path::new("/nonexistent"));
        assert!(result.is_err());
    }

    #[test]
    #[ignore] // Requires MLX runtime
    fn test_generate_basic() {
        let engine = LocalInferenceEngine::new(
            Path::new("./var/models/Qwen2.5-7B-Instruct")
        ).unwrap();
        let result = engine.generate("Hello", 10).unwrap();
        assert!(!result.is_empty());
    }
}
```

### Integration Tests

```rust
// tests/cli_local_chat_tests.rs

#[test]
#[ignore] // Requires model files
fn test_local_chat_single_prompt() {
    let output = Command::new("./aosctl")
        .args(&["chat", "prompt", "Hello", "--local", "--model-path", "./var/models/Qwen2.5-7B-Instruct"])
        .output()
        .expect("failed to run aosctl");

    assert!(output.status.success());
    assert!(!output.stdout.is_empty());
}

#[test]
fn test_local_chat_missing_model_path() {
    let output = Command::new("./aosctl")
        .args(&["chat", "prompt", "Hello", "--local"])
        .output()
        .expect("failed to run aosctl");

    assert!(!output.status.success());
    assert!(String::from_utf8_lossy(&output.stderr).contains("--model-path required"));
}
```

### Manual Tests

1. **Basic Local Chat**:
   ```bash
   ./aosctl chat interactive --local --model-path ./var/models/Qwen2.5-7B-Instruct
   # Type "Hello", verify response
   # Type "exit", verify clean shutdown
   ```

2. **Single Prompt**:
   ```bash
   ./aosctl chat prompt "What is 2+2?" --local --model-path ./var/models/Qwen2.5-7B-Instruct
   # Verify output is "4" or similar
   ```

3. **Error Handling**:
   ```bash
   ./aosctl chat interactive --local --model-path /nonexistent
   # Verify error message is clear
   ```

4. **Streaming Verification**:
   ```bash
   ./aosctl chat prompt "Write a haiku" --local --model-path ./var/models/Qwen2.5-7B-Instruct
   # Observe tokens appearing incrementally
   ```

---

## Dependencies

### Required Changes

| Crate | Change |
|-------|--------|
| `adapteros-cli` | Add `--local` flag, `LocalInferenceEngine` |
| `adapteros-lora-mlx-ffi` | None (already has `TextGenerationKernel`) |
| `adapteros-lora-kernel-api` | None (trait exists) |

### Feature Flags

```toml
# Cargo.toml (adapteros-cli)
[features]
local-chat = ["adapteros-lora-mlx-ffi"]  # Optional, but recommended
```

---

## Risks and Mitigations

| Risk | Impact | Mitigation |
|------|--------|------------|
| MLX FFI not initialized | Won't work | Clear error message + docs |
| Model loading slow | Poor UX | Progress indicator |
| OOM on large models | Crash | Memory check before loading |
| MoE models unsupported | Partial coverage | Fall back to subprocess |

---

## Timeline

This is scope, not schedule (no time estimates per project policy):

1. **Core implementation**: LocalInferenceEngine + CLI flags
2. **REPL integration**: Interactive mode with streaming
3. **Error handling**: Validation and clear messages
4. **Testing**: Unit + integration tests
5. **Documentation**: Update CLI_GUIDE.md

---

## Related Files

- `crates/adapteros-cli/src/commands/chat.rs` - Current chat command
- `crates/adapteros-cli/src/commands/mod.rs` - Command registry
- `crates/adapteros-lora-mlx-ffi/src/lib.rs` - MLX FFI backend
- `crates/adapteros-lora-kernel-api/src/lib.rs` - TextGenerationKernel trait
- `crates/adapteros-lora-worker/src/mlx_subprocess_bridge.rs` - Subprocess protocol
- `docs/CLI_GUIDE.md` - CLI documentation
- `docs/MLX_GUIDE.md` - MLX backend documentation
