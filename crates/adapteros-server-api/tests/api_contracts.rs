//! Compatibility target for API contract snapshots.
//!
//! This keeps `cargo test -p adapteros-server-api --test api_contracts` working
//! while the main implementation lives in `contract_snapshots.rs`.

include!("contract_snapshots.rs");
