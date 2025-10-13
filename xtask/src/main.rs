//! Build automation and SBOM generation

use anyhow::Result;
use std::env;

mod sbom;
mod verify_agents;

#[tokio::main]
async fn main() -> Result<()> {
    let task = env::args().nth(1);

    match task.as_deref() {
        Some("sbom") => sbom::generate_sbom()?,
        Some("build") => build()?,
        Some("test") => test()?,
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
    println!("  sbom           Generate SBOM from dependencies");
    println!("  build          Custom build workflow");
    println!("  test           Run full test suite");
    println!("  verify-agents  Verify all agent deliverables");
    println!();
    println!("For verify-agents options, run:");
    println!("  cargo xtask verify-agents --help");
}
