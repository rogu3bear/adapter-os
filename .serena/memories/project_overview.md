# AdapterOS Project Overview

## Purpose

AdapterOS is a Rust-based deterministic ML inference platform for Apple Silicon. It provides:

- **K-sparse LoRA routing** with Q15 quantized gates
- **Metal-optimized kernels** for GPU compute
- **CoreML ANE acceleration** for Apple Neural Engine
- **MLX FFI backend** for native macOS inference and training
- **Policy enforcement** for production environments
- **Deterministic replay** with seed isolation and audit trails

The system is designed for **air-gapped deployments with zero network egress** during serving.

## Tech Stack

| Layer | Technology |
|-------|------------|
| Language | Rust (stable channel) |
| Backend | Axum (async web framework) |
| Database | SQLite with migrations |
| Frontend | Leptos 0.7 + WASM (CSR) |
| GPU | Metal shaders (macOS) |
| ANE | CoreML acceleration |
| ML Runtime | MLX (C++ FFI) |
| Styling | Pure CSS (Liquid Glass design system) |
| Build | Cargo workspaces (83 crates) |
| WASM Bundler | Trunk |

## Key Constraints

1. **Determinism**: HKDF-SHA256 seed derivation, strict mode rejects missing seeds
2. **Air-gapped**: Zero network egress during serving
3. **Apple Silicon only**: Metal, CoreML, and MLX are macOS-specific
4. **Audit trails**: Merkle tree telemetry, atomic dual-write patterns

## Feature Flags

- `production-macos`: Full Apple Silicon stack (MLX + CoreML + Metal)
- `multi-backend`: MLX primary backend (default)
- `coreml-backend`: CoreML ANE acceleration (default)
- `metal-backend`: Metal GPU kernels
- `deterministic-only`: Enforce determinism (default)
