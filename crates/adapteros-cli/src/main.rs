//! AdapterOS CLI tool (aosctl)

#![allow(clippy::needless_borrow)]
#![allow(clippy::needless_borrows_for_generic_args)]
#![allow(noop_method_call)]
#![allow(clippy::unneeded_struct_pattern)]
#![allow(unused_imports)]
#![allow(unused_variables)]
#![allow(unused_mut)]
#![allow(unused_must_use)]
#![allow(private_interfaces)]
#![allow(clippy::too_many_arguments)]
#![allow(clippy::redundant_closure)]
#![allow(clippy::collapsible_else_if)]
#![allow(clippy::ptr_arg)]
#![allow(clippy::to_string_in_format_args)]
#![allow(dead_code)]
#![allow(clippy::only_used_in_recursion)]
#![allow(clippy::unnecessary_map_or)]
#![allow(clippy::unwrap_or_default)]
#![allow(clippy::manual_range_contains)]
#![allow(clippy::type_complexity)]
#![allow(clippy::useless_format)]
#![allow(clippy::len_zero)]
#![allow(clippy::useless_conversion)]
#![allow(clippy::useless_asref)]
#![allow(clippy::wildcard_in_or_patterns)]
#![allow(clippy::suspicious_doc_comments)]
#![allow(clippy::unnecessary_lazy_evaluations)]
#![allow(clippy::single_match)]

use adapteros_config::{BackendPreference, ModelConfig};
use anyhow::{bail, Result};
use clap::{CommandFactory, Parser, Subcommand};
use clap_complete::Shell;
use std::path::{Path, PathBuf};

mod cli;
mod cmd_replay;
mod cmd_trace_export;
pub mod utils;

// Use commands from library crate to avoid duplicate module compilation
use adapteros_cli::auth_store;
use adapteros_cli::cli_telemetry;
use adapteros_cli::commands;
use adapteros_cli::error_codes;
use adapteros_cli::formatting;
use adapteros_cli::http_client;
use adapteros_cli::local_inference;
use adapteros_cli::logging;
use adapteros_cli::output;

use adapteros_lora_worker::memory::{MemoryPressureLevel, UmaPressureMonitor};
use commands::golden::GoldenCmd;
use commands::init;
use commands::*;
use logging::init_logging;
use output::{OutputMode, OutputWriter};

// Use BackendType from library crate
use adapteros_cli::app::{
    extract_tenant_from_command, get_command_name, Cli, Commands, LogCommand, TraceCommand,
};
use adapteros_cli::BackendType;

#[tokio::main]
async fn main() -> Result<()> {
    // Load .env file first (before anything else reads env vars)
    adapteros_config::load_dotenv();
    // Preload env defaults from stored CLI login (if present)
    auth_store::preload_env_from_store();

    // Initialize unified logging
    init_logging()?;

    tracing::debug!(
        build_id = adapteros_core::version::BUILD_ID,
        git_commit = adapteros_core::version::GIT_COMMIT_HASH,
        version = adapteros_core::version::VERSION,
        profile = adapteros_core::version::BUILD_PROFILE,
        "aosctl starting"
    );

    let cli = Cli::parse();

    // Create output writer based on global flags
    let output_mode = OutputMode::from_flags(cli.is_json(), cli.is_quiet());
    let output = OutputWriter::new(output_mode, cli.is_verbose());

    // Get command name for telemetry
    let command_name = get_command_name(&cli.command);
    let tenant_id = extract_tenant_from_command(&cli.command);
    auth_store::warn_if_tenant_mismatch(tenant_id.as_deref(), &output);

    // Execute command and handle errors with telemetry
    let result = execute_command(&cli.command, &cli, &output).await;

    match result {
        Ok(_) => {
            // Emit success telemetry
            let _ =
                cli_telemetry::emit_cli_command(&command_name, tenant_id.as_deref(), true).await;
            Ok(())
        }
        Err(e) => {
            // Extract error code and emit telemetry
            let error_code = cli_telemetry::extract_error_code(&e);
            let error_msg = format!("{}", e);

            let event_id = cli_telemetry::emit_cli_error(
                error_code.as_deref(),
                &command_name,
                tenant_id.as_deref(),
                &error_msg,
            )
            .await
            .unwrap_or_else(|_| "-".to_string());

            // If error code exists, suggest using explain with event ID
            if let Some(code) = error_code {
                eprintln!(
                    "\n✗ {} – see: aosctl explain {} (event: {})",
                    code, code, event_id
                );
            }

            Err(e)
        }
    }
}

async fn execute_command(command: &Commands, cli: &Cli, output: &OutputWriter) -> Result<()> {
    match command {
        // Auth management
        Commands::Auth(cmd) => {
            commands::auth_cli::handle_auth_command(cmd.clone(), &output).await?;
        }

        // System Initialization (Owner Home)
        Commands::Init { args } => {
            init::run(args.clone(), output).await?;
        }

        // Tenant Management
        Commands::TenantInit { id, uid, gid } => {
            init_tenant::run(&id, *uid, *gid, &output).await?;
        }

        // Adapter Management
        Commands::Adapter(cmd) => {
            adapter::handle_adapter_command(cmd.clone(), &output).await?;
        }

        // Repository + Version Management
        Commands::Repo(cmd) => {
            commands::repo::run_repo_command(cmd.clone(), cli.is_json()).await?;
        }

        // Adapter Stack Management
        Commands::Stack(cmd) => {
            stack::handle_stack_command(cmd.clone(), &output).await?;
        }

        // Interactive Chat
        Commands::Chat(cmd) => {
            chat::handle_chat_command(cmd.clone(), &output).await?;
        }

        // Multi-Agent Operations
        Commands::Agent(cmd) => {
            commands::agent::handle_agent_command(cmd.clone(), &output).await?;
        }

        // Development Commands
        Commands::Dev { cmd } => {
            if let Some(inner) = cmd {
                dev::handle_dev_command(inner.clone(), &output).await?;
            } else {
                dev::dev_all(&output).await?;
            }
        }

        // Scenario readiness utilities
        #[cfg(feature = "scenarios")]
        Commands::Scenario(cmd) => {
            commands::scenario::run(cmd.clone(), &output).await?;
        }

        // CoreML verification status
        Commands::Coreml(cmd) => {
            commands::coreml_status::run(cmd.clone(), &output).await?;
        }

        // CoreML export pipeline
        #[cfg(feature = "coreml-export")]
        Commands::CoremlExport {
            base_package,
            adapter_aos,
            output_package,
            compute_units,
            base_model_id,
            adapter_id,
        } => {
            commands::coreml_export::run(
                base_package.clone(),
                adapter_aos.clone(),
                output_package.clone(),
                compute_units.clone(),
                base_model_id.clone(),
                adapter_id.clone(),
                &output,
            )
            .await?;
        }
        #[cfg(feature = "coreml-export")]
        Commands::CoremlExportJob { job_id, base_url } => {
            commands::coreml_export::trigger_export_for_job(job_id, base_url, &output).await?;
        }
        #[cfg(feature = "coreml-export")]
        Commands::CoremlExportStatus { job_id, base_url } => {
            commands::coreml_export::show_export_status(job_id, base_url, &output).await?;
        }

        // Node & Cluster Management
        Commands::Node(cmd) => {
            node::handle_node_command(cmd.clone(), &output).await?;
        }

        // Deployment
        Commands::Deploy(cmd) => {
            commands::deploy::run(cmd.clone(), &output).await?;
        }

        // System Status
        Commands::Status(cmd) => {
            commands::status::run(cmd.clone(), &output).await?;
        }

        // System Health Diagnostics
        Commands::Doctor(cmd) => {
            commands::doctor::run(cmd.clone(), &output).await?;
        }

        // Post-reboot Startup Verification
        Commands::Check(cmd) => {
            commands::check::run(cmd.clone(), &output).await?;
        }

        // Pre-flight System Readiness Check
        Commands::Preflight(cmd) => {
            commands::preflight::run(cmd.clone(), &output).await?;
        }

        // Maintenance
        Commands::Maintenance(cmd) => {
            commands::maintenance::run(cmd.clone(), &output).await?;
        }

        // Registry Management
        Commands::Registry(cmd) => {
            registry::handle_registry_command(cmd.clone(), &output).await?;
        }

        // Storage Management
        Commands::Storage(cmd) => {
            storage::handle_storage_command(cmd.clone(), &output).await?;
        }

        // Database Management
        Commands::Db(cmd) => {
            commands::db::handle_db_command(cmd.clone(), &output).await?;
        }

        // Review Management
        Commands::Review(cmd) => {
            commands::review::handle_review_command(cmd.clone(), &output).await?;
        }

        // Model Management
        Commands::Models(cmd) => {
            let model_path_override = cli.model_path.as_deref().map(PathBuf::from);
            commands::models::handle_models_command(cmd.clone(), &output, model_path_override)
                .await?;
        }

        // Plan Management
        Commands::PlanBuild {
            manifest,
            output: output_path,
            tenant_id,
        } => {
            build_plan::run(&manifest, &output_path, tenant_id.as_deref(), &output).await?;
        }

        // Model Management
        Commands::ModelImport {
            name,
            weights,
            config,
            tokenizer,
            tokenizer_cfg,
            license,
        } => {
            import_model::run(
                &name,
                &weights,
                &config,
                &tokenizer,
                &tokenizer_cfg,
                &license,
                &output,
            )
            .await?;
        }

        // Telemetry & Verification
        Commands::Telemetry(cmd) => {
            telemetry::handle_telemetry_command(cmd.clone(), &output).await?;
        }

        Commands::Trace(trace_cmd) => match trace_cmd {
            TraceCommand::Export {
                request,
                out,
                fixtures,
            } => {
                let expectation =
                    cmd_trace_export::run(request, out, fixtures.as_deref(), &output)?;
                if output.mode().is_json() {
                    output.print_json(&serde_json::to_value(&expectation)?)?;
                } else if output.is_verbose() {
                    output.progress(format!(
                        "Expected receipt: {}",
                        expectation.expected_receipt
                    ));
                }
            }
        },

        Commands::Federation(cmd) => {
            federation::handle_federation_command(cmd.clone(), &output).await?;
        }

        Commands::DriftCheck {
            config,
            dataset,
            manifest,
            backend,
            reference_backend,
        } => {
            std::process::exit(
                commands::drift_check::drift_check(commands::drift_check::DriftCheckArgs {
                    config: config.clone(),
                    dataset_override: dataset.clone(),
                    manifest_override: manifest.clone(),
                    backends_override: backend.clone(),
                    reference_backend: reference_backend.clone(),
                })
                .await?,
            );
        }

        // CodeGraph & Call Graph
        #[cfg(feature = "codegraph")]
        Commands::Codegraph(cmd) => {
            codegraph::handle_codegraph_command(cmd.clone(), &output).await?;
        }

        // Security Daemon
        #[cfg(feature = "secd-support")]
        Commands::Secd(cmd) => {
            secd::handle_secd_command(cmd.clone()).await?;
        }

        // General Operations
        Commands::Import { bundle, no_verify } => {
            import::run(&bundle, !no_verify, &output).await?;
        }
        Commands::Verify {
            target,
            trace,
            bundle,
            base_url,
        } => {
            let as_trace = *trace || (!bundle && !Path::new(target).exists());
            if as_trace {
                verify::verify_trace_receipt(target.clone(), base_url, &output).await?;
            } else {
                verify::run(Path::new(target), &output).await?;
            }
        }
        Commands::VerifyReceipt {
            bundle,
            online,
            server_url,
        } => {
            commands::verify_receipt::run(
                bundle.as_deref(),
                online.as_deref(),
                server_url,
                &output,
            )
            .await?;
        }
        Commands::VerifyCancellationReceipt {
            trace_id,
            file,
            expected_pubkey,
        } => {
            commands::verify_cancellation_receipt::run(
                trace_id.as_deref(),
                file.as_deref(),
                expected_pubkey.as_deref(),
                &output,
            )
            .await?;
        }
        Commands::VerifyDeterminismLoop => {
            let exit_code = verify_determinism_loop::run(&output).await?;
            std::process::exit(exit_code);
        }
        Commands::VerifyAdapters => {
            let exit_code = commands::verify_adapters::run(&output).await?;
            std::process::exit(exit_code);
        }

        // Operational Tooling
        Commands::Ops(cmd) => {
            commands::ops::handle_ops_command(cmd.clone(), &output).await?;
        }

        // Policy Management
        Commands::Policy(cmd) => {
            cmd.clone().run()?;
        }

        Commands::Serve {
            tenant,
            plan,
            socket,
            backend,
            dry_run,
            capture_events,
        } => {
            // Build model config from CLI flags (precedence: CLI > ENV > defaults)
            let model_config = cli.get_model_config().ok();
            serve::run(
                tenant,
                plan,
                socket,
                backend.clone(),
                *dry_run,
                capture_events.as_ref(), // capture_events
                model_config.as_ref(),
                &output,
            )
            .await?;
        }
        Commands::Audit { cpid, suite } => {
            audit::run(&cpid, suite.as_deref(), &output).await?;
        }
        Commands::AuditDeterminism { args } => {
            let audit_output = audit_determinism::Output;
            let exit_code = audit_determinism::run(args, &audit_output)?;
            std::process::exit(exit_code);
        }
        Commands::Infer {
            adapter,
            prompt,
            socket,
            max_tokens,
            require_evidence,
            timeout,
            show_citations,
            show_trace,
        } => {
            // Check UMA pressure before inference
            let monitor = UmaPressureMonitor::new(15, None);
            let pressure = monitor.get_current_pressure();
            if matches!(
                pressure,
                MemoryPressureLevel::High | MemoryPressureLevel::Critical
            ) {
                eprintln!(
                    "System under pressure (level: {}), retry in 30s or reduce max_tokens",
                    pressure.to_string()
                );
                std::process::exit(1);
            }

            commands::infer::run(
                adapter.clone(),
                prompt.clone(),
                *max_tokens,
                *require_evidence,
                socket.clone(),
                *timeout,
                *show_citations,
                *show_trace,
            )
            .await?;
        }
        Commands::Replay {
            dir,
            report,
            verify,
        } => {
            let availability = cmd_replay::generate_availability_report(dir, &output)?;
            if !availability.required_checks_passed {
                bail!(
                    "replay preflight failed: {} (see {})",
                    availability.blocking_failures.join("; "),
                    availability.report_path.display()
                );
            }

            let report_path = report.as_ref().map(|p| p.as_path());
            let replay_report = cmd_replay::run(dir, *verify, report_path, &availability, &output)?;

            if output.mode().is_json() {
                output.print_json(&serde_json::to_value(&replay_report)?)?;
            } else if output.is_verbose() {
                output.progress(format!("Replay status: {}", replay_report.status));
            }
        }
        Commands::Rollback { tenant, cpid } => {
            rollback::run(&tenant, &cpid, &output).await?;
        }
        Commands::Golden(cmd) => {
            golden::execute(cmd, &output).await?;
        }
        Commands::Router(cmd) => {
            cmd.clone().run()?;
        }
        Commands::Report {
            bundle,
            output: output_path,
        } => {
            report::run(&bundle, &output_path, &output).await?;
        }
        Commands::Bootstrap {
            mode,
            air_gapped,
            json,
            checkpoint_file,
        } => {
            // Bootstrap doesn't use OutputWriter, runs standalone
            bootstrap::run(&mode, *air_gapped, *json, checkpoint_file.clone()).await?;
        }

        // Utility
        Commands::Completions { shell } => {
            let mut cmd = Cli::command();
            completions::generate_completions(*shell, &mut cmd)?;
        }

        // Configuration Management
        Commands::Config(args) => {
            config::run_config_command(args.clone(), &output).await?;
        }

        // Backend Status
        Commands::BackendStatus(args) => {
            commands::backend_status::run(args.clone()).await?;
        }

        // Documentation & Help
        Commands::Diag(cmd) => {
            diag::handle_diag_command(cmd.clone(), &output).await?;
        }

        // Log analysis commands
        Commands::Log(cmd) => match cmd {
            LogCommand::Digest(args) => {
                commands::log_digest::run(args.clone(), &output).await?;
            }
            LogCommand::Triage(args) => {
                commands::log_triage::run(args.clone(), &output).await?;
            }
            LogCommand::Prompt(args) => {
                commands::log_prompt::run(args.clone(), &output).await?;
            }
        },

        Commands::Health(cmd) => {
            commands::diag_health::run(cmd.clone(), &output).await?;
        }

        Commands::Determinism {
            stack_id,
            runs,
            seed,
        } => {
            diag::run_determinism_check(stack_id.clone(), *runs, seed.clone(), &output).await?;
        }
        Commands::Quarantine { verbose } => {
            diag::run_quarantine_check(*verbose, &output).await?;
        }
        Commands::Explain { code } => {
            explain::explain(&code).await?;
        }

        Commands::ErrorCodes { json } => {
            explain::list_error_codes(*json).await?;
        }

        Commands::Tutorial { advanced, ci } => {
            commands::tutorial::run_tutorial(
                output.clone(),
                commands::tutorial::TutorialArgs {
                    advanced: *advanced,
                    ci: *ci,
                },
            )
            .await?;
        }

        // TUI Dashboard
        Commands::Tui { server_url } => {
            #[cfg(feature = "tui")]
            {
                commands::tui::run(commands::tui::TuiArgs {
                    server_url: server_url.clone(),
                })
                .await?;
            }
            #[cfg(not(feature = "tui"))]
            {
                let _ = server_url; // Suppress unused warning
                anyhow::bail!("TUI feature not enabled. Rebuild with: cargo build --features tui");
            }
        }

        Commands::Manual { args } => {
            commands::manual::run_manual(args.clone())?;
        }

        Commands::Train(cmd) => {
            commands::train_cli::run(cmd.clone(), &output).await?;
        }

        Commands::Dataset(cmd) => {
            commands::datasets::run(cmd.clone(), &output).await?;
        }

        Commands::TrainDocs { args } => {
            args.execute().await?;
        }

        Commands::TrainEmbeddings { args } => {
            train_embeddings::run(args.clone()).await?;
        }

        Commands::TrainFromDiscrepancies { args } => {
            args.execute(&output).await?;
        }

        // Embedding Benchmark Commands
        Commands::Embed(cmd) => {
            commands::embed::handle_embed_command(cmd.clone()).await?;
        }

        // Code Intelligence Commands
        Commands::Code(cmd) => {
            code::handle_code_command(cmd.clone(), &output).await?;
        }

        // ============================================================
        // Deprecated Commands (backward compatibility)
        // ============================================================
        Commands::AdapterListDeprecated { .. } => {
            eprintln!("Warning: 'adapter-list' is deprecated. Use 'aosctl adapter list' instead.");
            adapter::handle_adapter_command(
                adapter::AdapterCommand::List {
                    json: cli.is_json(),
                    tenant: None,
                    pinned_only: false,
                },
                &output,
            )
            .await?;
        }

        Commands::AdapterPinDeprecated { adapter_id, tenant } => {
            eprintln!("Warning: 'adapter-pin' is deprecated. Use 'aosctl adapter pin' instead.");
            adapter::handle_adapter_command(
                adapter::AdapterCommand::Pin {
                    adapter_id: adapter_id.clone(),
                    tenant: tenant.clone(),
                },
                &output,
            )
            .await?;
        }

        Commands::AdapterUnpinDeprecated { adapter_id, tenant } => {
            eprintln!(
                "Warning: 'adapter-unpin' is deprecated. Use 'aosctl adapter unpin' instead."
            );
            adapter::handle_adapter_command(
                adapter::AdapterCommand::Unpin {
                    adapter_id: adapter_id.clone(),
                    tenant: tenant.clone(),
                },
                &output,
            )
            .await?;
        }

        Commands::NodeListDeprecated { offline } => {
            eprintln!("Warning: 'node-list' is deprecated. Use 'aosctl node list' instead.");
            node::handle_node_command(
                node::NodeCommand::List {
                    offline: *offline,
                    json: cli.is_json(),
                },
                &output,
            )
            .await?;
        }

        Commands::NodeVerifyDeprecated { all, nodes } => {
            eprintln!("Warning: 'node-verify' is deprecated. Use 'aosctl node verify' instead.");
            node::handle_node_command(
                node::NodeCommand::Verify {
                    all: *all,
                    nodes: nodes.clone(),
                    json: cli.is_json(),
                },
                &output,
            )
            .await?;
        }

        Commands::TelemetryListDeprecated {
            database,
            by_stack,
            limit,
        } => {
            eprintln!(
                "Warning: 'telemetry-list' is deprecated. Use 'aosctl telemetry list' instead."
            );
            telemetry::handle_telemetry_command(
                telemetry::TelemetryCommand::List {
                    database: database.clone(),
                    by_stack: by_stack.clone(),
                    event_type: None,
                    limit: *limit,
                },
                &output,
            )
            .await?;
        }

        Commands::TelemetryVerifyDeprecated { bundle_dir } => {
            eprintln!(
                "Warning: 'telemetry-verify' is deprecated. Use 'aosctl telemetry verify' instead."
            );
            telemetry::handle_telemetry_command(
                telemetry::TelemetryCommand::Verify {
                    bundle_dir: bundle_dir.clone(),
                },
                &output,
            )
            .await?;
        }

        Commands::RegistrySyncDeprecated {
            dir,
            cas_root,
            registry: registry_path,
        } => {
            eprintln!(
                "Warning: 'registry-sync' is deprecated. Use 'aosctl registry sync' instead."
            );
            registry::handle_registry_command(
                registry::RegistryCommand::Sync {
                    dir: dir.clone(),
                    public_key: None,
                    cas_root: cas_root.clone(),
                    registry: registry_path.clone(),
                },
                &output,
            )
            .await?;
        }

        Commands::RegistryMigrateDeprecated {
            from_db,
            to_db,
            dry_run,
            force,
        } => {
            eprintln!(
                "Warning: 'registry-migrate' is deprecated. Use 'aosctl registry migrate' instead."
            );
            registry::handle_registry_command(
                registry::RegistryCommand::Migrate(registry::RegistryMigrateArgs {
                    from_db: from_db.clone(),
                    to_db: to_db.clone(),
                    dry_run: *dry_run,
                    force: *force,
                }),
                &output,
            )
            .await?;
        }

        Commands::FederationVerifyDeprecated {
            bundle_dir,
            database,
        } => {
            eprintln!("Warning: 'federation-verify' is deprecated. Use 'aosctl federation verify' instead.");
            federation::handle_federation_command(
                federation::FederationCommand::Verify {
                    bundle_dir: bundle_dir.clone(),
                    database: database.clone(),
                },
                &output,
            )
            .await?;
        }

        Commands::CodeInitDeprecated { repo_path, tenant } => {
            eprintln!("Warning: 'code-init' is deprecated. Use 'aosctl code init' instead.");
            code::handle_code_command(
                code::CodeCommand::Init {
                    repo_path: repo_path.clone(),
                    tenant: tenant.clone(),
                },
                &output,
            )
            .await?;
        }

        Commands::CodeUpdateDeprecated {
            repo_id,
            tenant,
            commit,
        } => {
            eprintln!("Warning: 'code-update' is deprecated. Use 'aosctl code update' instead.");
            code::handle_code_command(
                code::CodeCommand::Update {
                    repo_id: repo_id.clone(),
                    tenant: tenant.clone(),
                    commit: commit.clone(),
                },
                &output,
            )
            .await?;
        }

        Commands::CodeListDeprecated { tenant } => {
            eprintln!("Warning: 'code-list' is deprecated. Use 'aosctl code list' instead.");
            code::handle_code_command(
                code::CodeCommand::List {
                    tenant: tenant.clone(),
                },
                &output,
            )
            .await?;
        }

        Commands::CodeStatusDeprecated { repo_id, tenant } => {
            eprintln!("Warning: 'code-status' is deprecated. Use 'aosctl code status' instead.");
            code::handle_code_command(
                code::CodeCommand::Status {
                    repo_id: repo_id.clone(),
                    tenant: tenant.clone(),
                },
                &output,
            )
            .await?;
        }

        #[cfg(feature = "secd-support")]
        Commands::SecdStatusDeprecated {
            pid_file,
            heartbeat_file,
            socket,
            database,
        } => {
            eprintln!("Warning: 'secd-status' is deprecated. Use 'aosctl secd status' instead.");
            secd::handle_secd_command(secd::SecdCommand::Status {
                pid_file: pid_file.clone(),
                heartbeat_file: heartbeat_file.clone(),
                socket: socket.clone(),
                database: database.clone(),
            })
            .await?;
        }

        #[cfg(feature = "secd-support")]
        Commands::SecdAuditDeprecated {
            database,
            limit,
            operation,
        } => {
            eprintln!("Warning: 'secd-audit' is deprecated. Use 'aosctl secd audit' instead.");
            secd::handle_secd_command(secd::SecdCommand::Audit {
                database: database.clone(),
                limit: *limit,
                operation: operation.clone(),
            })
            .await?;
        }

        #[cfg(feature = "codegraph")]
        Commands::CodegraphStatsDeprecated { codegraph_db } => {
            eprintln!(
                "Warning: 'codegraph-stats' is deprecated. Use 'aosctl codegraph stats' instead."
            );
            codegraph::handle_codegraph_command(
                codegraph::CodegraphCommand::Stats {
                    codegraph_db: codegraph_db.clone(),
                },
                &output,
            )
            .await?;
        }

        #[cfg(feature = "codegraph")]
        Commands::CallgraphExportDeprecated {
            codegraph_db,
            output: output_path,
            format,
        } => {
            eprintln!(
                "Warning: 'callgraph-export' is deprecated. Use 'aosctl codegraph export' instead."
            );
            codegraph::handle_codegraph_command(
                codegraph::CodegraphCommand::Export {
                    codegraph_db: codegraph_db.clone(),
                    output: output_path.clone(),
                    format: format.clone(),
                },
                &output,
            )
            .await?;
        }
    }

    Ok(())
}
// Logging initialization moved to logging module
