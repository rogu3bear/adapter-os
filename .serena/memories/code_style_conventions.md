# Code Style and Conventions

## General Rust Style

- Follow standard Rust conventions (rustfmt enforced)
- Run `cargo fmt --all` before committing
- Run `cargo clippy --workspace -- -D warnings` for lints

## Naming Conventions

- Crates: `adapteros-{domain}` (e.g., `adapteros-lora-router`, `adapteros-db`)
- Modules: snake_case
- Types: PascalCase
- Functions/methods: snake_case
- Constants: SCREAMING_SNAKE_CASE

## Error Handling

- Use `thiserror` for error types
- Error codes are enforced at the API level
- See `crates/adapteros-core/src/error.rs` for patterns

## API Types

- Shared types in `adapteros-api-types` crate
- Use `wasm` feature for WASM-compatible types
- Serde for serialization

## Determinism Rules (Critical)

- Seed derivation: HKDF-SHA256 with BLAKE3 global seed
- Router tie-breaking: score DESC, index ASC
- Q15 quantization denominator: 32767.0
- **No `-ffast-math` compiler flags**
- Set `AOS_DEBUG_DETERMINISM=1` to debug

## Leptos UI Conventions

- Components in `src/components/`
- Pages in `src/pages/`
- Use `#[component]` macro
- Signal-based reactivity
- Follow Liquid Glass design system (see `dist/glass.css`)

## File Organization

- Unit tests in same file as code (`#[cfg(test)]` modules)
- Integration tests in `tests/` directories
- Workspace integration tests in repo root `tests/`

## Documentation

- Doc comments for public APIs
- Use `///` for item docs
- Use `//!` for module docs
- Keep comments minimal; prefer self-documenting code
