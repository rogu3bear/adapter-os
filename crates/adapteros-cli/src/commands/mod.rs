//! CLI command implementations

pub mod adapter;
pub mod adapter_info;
pub mod adapter_swap;
pub mod audit;
pub mod bootstrap;
pub mod build_plan;
pub mod cdp_list;
pub mod completions;
pub mod diag;
// pub mod diag_bundle;  // Temporarily disabled - functionality moved to diag.rs
pub mod explain;
// pub mod export_callgraph;  // Temporarily disabled due to mplora-codegraph dependency
pub mod import;
pub mod import_model;
pub mod init_tenant;
pub mod list_adapters;
pub mod manual;
pub mod metrics;
pub mod node_list;
pub mod node_sync;
pub mod node_verify;
pub mod pin;
pub mod policy;
pub mod profile;
pub mod register_adapter;
pub mod replay;
pub mod report;
pub mod rollback;
pub mod router;
pub mod secd_audit;
pub mod secd_status;
pub mod serve;
pub mod sync_registry;
pub mod telemetry_show;
pub mod trace;
pub mod tutorial;
pub mod verify;
pub mod verify_telemetry;
pub mod golden;
