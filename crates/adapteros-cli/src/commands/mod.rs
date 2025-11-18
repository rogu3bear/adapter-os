//! CLI command implementations

pub mod adapter;
pub mod adapter_info;
pub mod adapter_swap;
pub mod audit;
pub mod audit_determinism;
pub mod bootstrap;
pub mod build_plan;
pub mod cdp_list;
pub mod code;
pub mod completions;
pub mod db;
pub mod deploy;
pub mod diag;
pub mod doctor;
// pub mod diag_bundle;  // Temporarily disabled - functionality moved to diag.rs
pub mod drift_check;
pub mod explain;
// pub mod export_callgraph;  // Temporarily disabled due to mplora-codegraph dependency
pub mod golden;
pub mod import;
pub mod import_model;
pub mod ingest_docs;
pub mod init_tenant;
pub mod list_adapters;
pub mod maintenance;
pub mod manual;
pub mod status;
// pub mod metrics;  // Temporarily disabled - depends on adapteros-system-metrics
pub mod node_list;
pub mod node_sync;
pub mod node_verify;
pub mod pin;
pub mod policy;
pub mod profile;
pub mod register_adapter;
pub mod registry_migrate;
pub mod replay;
pub mod report;
pub mod rollback;
pub mod router;
pub mod secd_audit;
pub mod secd_status;
pub mod serve;
pub mod sync_registry;
pub mod telemetry_list;
// pub mod telemetry_show;  // TODO: Implement telemetry_show command
pub mod infer;
pub mod trace;
pub mod train;
pub mod tutorial;
pub mod verify;
pub mod verify_adapter;
pub mod verify_adapters;
pub mod verify_determinism_loop;
pub mod verify_federation;
pub mod verify_gpu;
pub mod verify_telemetry;
