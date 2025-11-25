Agent Metal Backend Verification (Deterministic)
=================================================

Purpose: give agents a deterministic, sandbox-safe checklist to verify the Metal backend without touching build scripts.

What is embedded
----------------
- The embedded metallib is `crates/adapteros-lora-kernel-mtl/shaders/aos_kernels.metallib` (this is the one shipped with the crate).
- Hash reference lives at `crates/adapteros-lora-kernel-mtl/manifests/metallib_manifest.json` (`kernel_hash`).

Verification steps
------------------
1) Build (Cargo path, preferred in sandboxes)
- `cargo build -p adapteros-lora-kernel-mtl --features metal-backend`
- Use Cargo instead of the Makefile; Makefile targets may fail in agent sandboxes when tool discovery (e.g., xcrun) is blocked. We are not changing build.rs/build.sh/Makefile.
- `xcrun --find metallib` may return nothing; treat as informational, not an error.

2) Hash the embedded metallib
- `b3sum crates/adapteros-lora-kernel-mtl/shaders/aos_kernels.metallib`
- Compare the output to `kernel_hash` in `crates/adapteros-lora-kernel-mtl/manifests/metallib_manifest.json`. Current reference: `37c3cac121d4b42d111ef83c421f768261585a2f70902b77ece79444bd031dfe`.

3) Classify tests
- Command: `cargo test -p adapteros-lora-kernel-mtl --features metal-backend`
- Expected environment failures in headless/agent runs: tests that require a Metal-capable device (e.g., cannot create `MTLDevice`, Metal validation layer missing). Classify these as device-dependent, not code defects.
- Treat panics, logic mismatches, or hash divergence as defects.

Optional workspace-local hint
-----------------------------
- For faster incremental builds without polluting global config, you may set:
  - `export CLANG_MODULE_CACHE_PATH="$PWD/target/clang-module-cache"`
- Place this in a workspace-local file such as `.envrc.example` (not auto-sourced) if you want persistence.
