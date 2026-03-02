# Temporarily Disabled Tools and Agent Artifacts

Based on a codebase search, here are the temporarily disabled tools and artifacts that coding agents likely created.

## 1. Temporarily Disabled Tools & Features

- **MLX Model Import**: The CLI command for importing MLX models is disabled (`crates/adapteros-cli/src/commands/import_model.rs`: "MLX model import is temporarily disabled - requires MLX C++ library").
- **Migrate Adapter Command**: The CLI command for migrating adapters is disabled (`crates/adapteros-cli/src/commands/migrate.rs`: "migrate adapter command is temporarily disabled").
- **Local LLM Backend**: The local LLM backend using MLX is disabled (`crates/adapteros-lora-worker/src/llm_backend.rs`: "Local LLM backend using MLX (temporarily disabled)").
- **Determinism Guards Initialization**: Worker logs show determinism guards are disabled (`Determinism guards initialization temporarily disabled due to dependency issues`).
- **Testing for `adapteros-db`**: The DB tests are disabled in the testing crate (`crates/adapteros-testing/Cargo.toml`: "# Note: adapteros-db temporarily disabled due to compilation errors").
- **Integration Tests**: 8 specific integration tests are disabled pending API refactoring (`tests/README.md`: "These tests are temporarily disabled pending API refactoring").
- **MLX Circuit Breaker**: There's a runtime circuit breaker that can temporarily disable a model (`crates/adapteros-lora-mlx-ffi/src/lib.rs`: "Circuit breaker open - model temporarily disabled").

## 2. Artifacts Created by Coding Agents

The project contains a specific directory (`.agents/workflows/`) which holds markdown artifacts detailing operational rules and guardrails for coding agents to follow. These artifacts were likely created by agents during previous tasks (such as "Preventing Concurrent Cargo Operations"):

- **`cargo-guardrail.md`**: A workflow artifact that instructs agents to run `./scripts/cargo-guard.sh` to check for active locks before running `cargo build` or `cargo check`. This prevents concurrent execution conflicts.
- **`cargo-optimization.md`**: A workflow artifact that instructs agents to minimize `cargo check` usage, use specific crate targets, and use the `./aosctl` script instead of `cargo run` to prevent blocking `rust-analyzer` and hanging the `adapter-os` workspace.
