# Workspace-Level Integration Tests

This directory enables workspace-level integration test discovery by Cargo.

## Purpose

The [`lib.rs`](lib.rs) file re-exports key modules needed by integration tests located in [`tests/`](../tests/).

## Why This Exists

Cargo's test discovery mechanism looks for a `src/lib.rs` file at the workspace root to enable integration tests to access workspace-level APIs. This is distinct from individual crate tests.

## Contents

- **`lib.rs`** - Re-exports core types like `FusedKernels` for test access

## Relationship to Crates

This is NOT a crate itself. It's a test support file. All actual crates are in [`crates/`](../crates/).

## Actual Tests

Integration tests are located in:

- [`tests/`](../tests/) - Workspace integration tests
- [`fuzz/`](../fuzz/) - Fuzzing targets
- `crates/*/tests/` - Per-crate unit tests

## Usage

This file is automatically used by `cargo test` at the workspace level. You don't need to interact with it directly.
