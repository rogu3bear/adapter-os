//! CLI command implementations

pub mod adapter;
pub mod adapter_info;
pub mod adapter_swap;
pub mod adapters;
pub mod aos;
pub mod aos_impl;
pub mod audit;
pub mod audit_determinism;
pub mod backend_status;
pub mod bootstrap;
pub mod build_plan;
pub mod cdp_list;
pub mod chat;
pub mod check;
pub mod code;
pub mod codegraph;
pub mod completions;
pub mod config;
pub mod db;
pub mod deploy;
pub mod dev;
pub mod diag;
pub mod doctor;
// pub mod diag_bundle;  // Temporarily disabled - functionality moved to diag.rs
pub mod drift_check;
pub mod explain;
pub mod federation;
// pub mod export_callgraph;  // Temporarily disabled due to mplora-codegraph dependency
pub mod golden;
pub mod import;
pub mod import_model;
pub mod ingest_docs;
pub mod init;
pub mod init_tenant;
pub mod list_adapters;
pub mod maintenance;
pub mod manual;
pub mod stack;
pub mod status;
// pub mod metrics;  // Temporarily disabled - depends on adapteros-system-metrics
pub mod infer;
pub mod node;
// Legacy node commands - still used by app.rs standalone commands
// TODO: Migrate app.rs Commands::NodeList/NodeVerify/NodeSync to use node.rs subcommands
pub mod node_list;
pub mod node_sync;
pub mod node_verify;
pub mod pin;
pub mod policy;
pub mod preflight;
pub mod preflight_fix;
pub mod profile;
pub mod register_adapter;
pub mod registry;
// Legacy registry commands - consolidated into registry.rs
// pub mod registry_migrate;  // Consolidated into registry.rs
// pub mod sync_registry;     // Consolidated into registry.rs
pub mod replay;
pub mod report;
pub mod rollback;
pub mod router;
pub mod secd;
#[cfg(feature = "secd-support")]
pub mod secd_audit;
#[cfg(feature = "secd-support")]
pub mod secd_status;
pub mod serve;
pub mod telemetry;
#[cfg(feature = "tui")]
pub mod tui;
// Legacy telemetry commands - consolidated into telemetry.rs
// pub mod telemetry_list;     // Consolidated into telemetry.rs
// pub mod verify_telemetry;   // Consolidated into telemetry.rs
pub mod telemetry_show;
pub mod trace;
pub mod train;
pub mod train_docs;
pub mod tutorial;
pub mod verify;
pub mod verify_adapter;
pub mod verify_adapters;
pub mod verify_determinism_loop;
// Legacy federation command - consolidated into federation.rs
// pub mod verify_federation;  // Consolidated into federation.rs
pub mod verify_gpu;
