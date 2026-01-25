# Workspace-Level Integration Tests

This directory enables workspace-level integration test discovery by Cargo.

## Purpose

The [`lib.rs`](file:///Users/star/Dev/adapter-os/src/lib.rs) file re-exports key modules needed by integration tests located in [`tests/`](file:///Users/star/Dev/adapter-os/tests).

## Why This Exists

Cargo's test discovery mechanism looks for a `src/lib.rs` file at the workspace root to enable integration tests to access workspace-level APIs. This is distinct from individual crate tests.

## Contents

- **`lib.rs`** - Re-exports core types like `FusedKernels` for test access

## Relationship to Crates

This is NOT a crate itself. It's a test support file. All actual crates are in [`crates/`](file:///Users/star/Dev/adapter-os/crates).

## Actual Tests

Integration tests are located in:

- [`tests/`](file:///Users/star/Dev/adapter-os/tests) - Workspace integration tests
- [`fuzz/`](file:///Users/star/Dev/adapter-os/fuzz) - Fuzzing targets
- `crates/*/tests/` - Per-crate unit tests

## Usage

This file is automatically used by `cargo test` at the workspace level. You don't need to interact with it directly.
