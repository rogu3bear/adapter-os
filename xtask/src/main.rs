//! Build automation and developer workflows for AdapterOS

use anyhow::Result;
use std::env;

mod check_all;
mod check_cache;
mod code2db_dataset;
mod determinism_report;
mod openapi_docs;
mod pack_lora;
mod sbom;
mod train_base_adapter;
mod verify_adapters;
mod verify_artifacts;

#[tokio::main]
async fn main() -> Result<()> {
    let task = env::args().nth(1);

    match task.as_deref() {
        Some("check-all") => {
            let verbose = env::args().any(|arg| arg == "--verbose" || arg == "-v");
            check_all::run(verbose)?;
        }
        Some("check-cache") => check_cache::run()?,
        Some("sbom") => sbom::generate_sbom()?,
        Some("determinism-report") => determinism_report::generate_determinism_report()?,
        Some("verify-artifacts") => verify_artifacts::run()?,
        Some("openapi-docs") => openapi_docs::run()?,
        Some("code2db-dataset") => {
            use clap::Parser;
            let args_vec: Vec<String> = env::args().collect();
            let parsed = if args_vec.len() > 1 {
                let mut new_args = vec![args_vec[0].clone()];
                new_args.extend(args_vec[2..].to_vec());
                code2db_dataset::Code2DbDatasetArgs::parse_from(new_args)
            } else {
                code2db_dataset::Code2DbDatasetArgs::parse()
            };
            code2db_dataset::run(parsed).await?;
        }
        Some("pack-lora") => {
            use clap::Parser;
            let args_vec: Vec<String> = env::args().collect();
            let parsed = if args_vec.len() > 1 {
                let mut new_args = vec![args_vec[0].clone()];
                new_args.extend(args_vec[2..].to_vec());
                pack_lora::PackLoraArgs::parse_from(new_args)
            } else {
                pack_lora::PackLoraArgs::parse()
            };
            pack_lora::run(parsed).await?;
        }
        Some("train-base-adapter") => {
            use clap::Parser;
            let args_vec: Vec<String> = env::args().collect();
            let parsed = if args_vec.len() > 1 {
                let mut new_args = vec![args_vec[0].clone()];
                new_args.extend(args_vec[2..].to_vec());
                train_base_adapter::TrainBaseAdapterArgs::parse_from(new_args)
            } else {
                train_base_adapter::TrainBaseAdapterArgs::parse()
            };
            train_base_adapter::run(parsed).await?;
        }
        Some("verify-adapters") => {
            // Parse args for verify-adapters subcommand
            use clap::Parser;
            // Skip first two args (program name and "verify-adapters")
            let args_vec: Vec<String> = env::args().collect();
            let verify_args = if args_vec.len() > 1 {
                // Prepend program name for clap
                let mut new_args = vec![args_vec[0].clone()];
                new_args.extend(args_vec[2..].to_vec());
                verify_adapters::VerifyAgentsArgs::parse_from(new_args)
            } else {
                verify_adapters::VerifyAgentsArgs::parse()
            };

            let report = verify_adapters::run(verify_args).await?;

            // Print summary
            println!("\n=== Verification Complete ===");
            println!("PASS: {}", report.summary.pass);
            println!("FAIL: {}", report.summary.fail);
            println!("SKIP: {}", report.summary.skip);

            // Exit with appropriate code
            std::process::exit(report.exit_code());
        }
        _ => print_help(),
    }

    Ok(())
}

fn print_help() {
    println!("AdapterOS Build Tasks");
    println!();
    println!("USAGE:");
    println!("  cargo xtask <TASK>");
    println!();
    println!("TASKS:");
    println!("  check-all           Run feature matrix checks (PRD-12)");
    println!("  check-cache         Check cache staleness (warns if >48h or >50GB)");
    println!("  sbom                Generate SBOM from dependencies");
    println!("  determinism-report  Generate build reproducibility report");
    println!("  verify-artifacts    Verify and sign release artifacts");
    println!("  openapi-docs        Generate OpenAPI documentation markdown");
    println!("  verify-adapters     Verify all adapter deliverables and proofs");
    println!("  code2db-dataset     Build JSON training dataset for code→DB tasks");
    println!("  pack-lora           Quantize and package trained LoRA weights");
    println!("  train-base-adapter  Train base adapter from manifest");
    println!();
    println!("For check-all options:");
    println!("  cargo xtask check-all [--verbose|-v]");
    println!("For verify-adapters options, run:");
    println!("  cargo xtask verify-adapters --help");
    println!("For dataset builder options, run:");
    println!("  cargo xtask code2db-dataset --help");
    println!("For LoRA packager options, run:");
    println!("  cargo xtask pack-lora --help");
    println!("For base adapter training options, run:");
    println!("  cargo xtask train-base-adapter --help");
}
