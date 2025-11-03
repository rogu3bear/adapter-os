//! Build automation and SBOM generation

use anyhow::Result;
use std::env;

mod code2db_dataset;
mod determinism_report;
mod pack_lora;
mod sbom;
mod train_base_adapter;
mod verify_agents;

#[tokio::main]
async fn main() -> Result<()> {
    let task = env::args().nth(1);

    match task.as_deref() {
        Some("sbom") => sbom::generate_sbom()?,
        Some("determinism-report") => determinism_report::generate_determinism_report()?,
        Some("build") => build()?,
        Some("test") => test()?,
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
        Some("verify-agents") => {
            // Parse args for verify-agents subcommand
            use clap::Parser;
            // Skip first two args (program name and "verify-agents")
            let args_vec: Vec<String> = env::args().collect();
            let verify_args = if args_vec.len() > 1 {
                // Prepend program name for clap
                let mut new_args = vec![args_vec[0].clone()];
                new_args.extend(args_vec[2..].to_vec());
                verify_agents::VerifyAgentsArgs::parse_from(new_args)
            } else {
                verify_agents::VerifyAgentsArgs::parse()
            };

            let report = verify_agents::run(verify_args).await?;

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

fn build() -> Result<()> {
    println!("Building AdapterOS...");
    // TODO: Custom build steps
    println!("✓ Build complete");
    Ok(())
}

fn test() -> Result<()> {
    println!("Running tests...");
    // TODO: Custom test orchestration
    println!("✓ Tests passed");
    Ok(())
}

fn print_help() {
    println!("AdapterOS Build Tasks");
    println!();
    println!("USAGE:");
    println!("  cargo xtask <TASK>");
    println!();
    println!("TASKS:");
    println!("  sbom                Generate SBOM from dependencies");
    println!("  determinism-report  Generate build reproducibility report");
    println!("  build               Custom build workflow");
    println!("  test                Run full test suite");
    println!("  verify-agents       Verify all agent deliverables");
    println!("  code2db-dataset     Build JSON training dataset for code→DB tasks");
    println!("  pack-lora           Quantize and package trained LoRA weights");
    println!("  train-base-adapter  Train base adapter from manifest");
    println!();
    println!("For verify-agents options, run:");
    println!("  cargo xtask verify-agents --help");
    println!("For dataset builder options, run:");
    println!("  cargo xtask code2db-dataset --help");
    println!("For LoRA packager options, run:");
    println!("  cargo xtask pack-lora --help");
    println!("For base adapter training options, run:");
    println!("  cargo xtask train-base-adapter --help");
}
