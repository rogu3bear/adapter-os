# CLI Chat MVP - Implementation Checklist

**Tracking file for**: docs/cli-chat-mvp.md
**Created**: 2026-01-05
**Status**: Not Started

---

## Implementation Tasks

### Phase 1: Core Infrastructure

- [ ] Add `--local` flag to `ChatArgs` in `crates/adapteros-cli/src/commands/chat.rs`
- [ ] Add `--model-path` flag (required when `--local` is set)
- [ ] Add `--backend` flag with enum `LocalBackend { Mlx, MlxSubprocess }`
- [ ] Add `--temperature`, `--top-p`, `--max-tokens` flags
- [ ] Create `crates/adapteros-cli/src/local_inference.rs` module

### Phase 2: LocalInferenceEngine

- [ ] Implement `LocalInferenceEngine::new(model_path: &Path)`
- [ ] Initialize MLX runtime via `adapteros_lora_mlx_ffi::mlx_runtime_init()`
- [ ] Load model via MLX FFI backend
- [ ] Implement `generate()` using `TextGenerationKernel::generate_text_full()`
- [ ] Implement `generate_stream()` using `TextGenerationKernel::generate_text_stream()`
- [ ] Add error handling for missing model path, MLX init failure

### Phase 3: REPL Integration

- [ ] Update `run_interactive()` to check `args.local`
- [ ] Implement `run_interactive_local()` with:
  - [ ] Model loading with progress indicator
  - [ ] Input loop with rustyline
  - [ ] Streaming output (print tokens as they arrive)
  - [ ] Clean exit on Ctrl-C/Ctrl-D and "exit" command

### Phase 4: Single Prompt Mode

- [ ] Update `run_prompt()` to check `args.local`
- [ ] Implement `run_prompt_local()` with:
  - [ ] Model loading
  - [ ] Single generation call
  - [ ] Print result to stdout

### Phase 5: Error Handling

- [ ] Validate model path exists before loading
- [ ] Check MLX runtime availability
- [ ] Memory check before model loading (optional)
- [ ] Clear error messages for common failures

### Phase 6: Testing

- [ ] Unit test: model path validation
- [ ] Unit test: generate basic (ignored, requires MLX)
- [ ] Integration test: local chat single prompt
- [ ] Integration test: missing model path error
- [ ] Manual test: REPL mode streaming
- [ ] Manual test: graceful shutdown

### Phase 7: Documentation

- [ ] Update `docs/CLI_GUIDE.md` with local mode section
- [ ] Add examples to CLAUDE.md if appropriate
- [ ] Document `AOS_MODEL_PATH` environment variable

---

## Key Files to Modify

| File | Change |
|------|--------|
| `crates/adapteros-cli/src/commands/chat.rs` | Add flags, dispatch to local mode |
| `crates/adapteros-cli/src/commands/mod.rs` | Export local_inference if needed |
| `crates/adapteros-cli/src/local_inference.rs` | **NEW**: LocalInferenceEngine |
| `crates/adapteros-cli/Cargo.toml` | Add `adapteros-lora-mlx-ffi` dependency |
| `docs/CLI_GUIDE.md` | Document local mode |

---

## Dependencies to Verify

- [ ] `adapteros-lora-mlx-ffi` implements `TextGenerationKernel`
- [ ] `adapteros-lora-kernel-api::TextGenerationKernel` trait is public
- [ ] MLX runtime can be initialized from CLI context
- [ ] Tokenizer loading works without full Worker

---

## Blocking Issues

_None identified yet_

---

## Notes for Implementers

1. **MLX FFI Backend**: The key entry point is `adapteros_lora_mlx_ffi`. Check `src/lib.rs` for initialization patterns.

2. **TextGenerationKernel**: This trait at `adapteros-lora-kernel-api/src/lib.rs:1135` provides:
   - `generate_text_full()` - Non-streaming
   - `generate_text_stream()` - Streaming via callback

3. **Existing Chat Implementation**: Study `chat.rs` lines 180-300 for the HTTP-based REPL pattern. The local version should mirror this UX.

4. **Feature Flag**: Consider adding `local-chat` feature to make MLX dependency optional for minimal CLI builds.

5. **Error Recovery**: If MLX init fails, provide actionable error messages pointing to `docs/MLX_GUIDE.md`.

---

## Sign-off

| Phase | Completed | Agent/PR |
|-------|-----------|----------|
| Phase 1 | [ ] | |
| Phase 2 | [ ] | |
| Phase 3 | [ ] | |
| Phase 4 | [ ] | |
| Phase 5 | [ ] | |
| Phase 6 | [ ] | |
| Phase 7 | [ ] | |
