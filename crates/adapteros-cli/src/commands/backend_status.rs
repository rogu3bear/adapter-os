//! Backend status command - shows available backends and their capabilities

use clap::Args;
use std::process;

/// Show backend status and capabilities
#[derive(Args)]
pub struct BackendStatusArgs {
    /// Show detailed information about each backend
    #[arg(long)]
    detailed: bool,

    /// Output in JSON format
    #[arg(long)]
    json: bool,
}

pub async fn run(args: BackendStatusArgs) -> anyhow::Result<()> {
    if args.json {
        // Output JSON format
        let backends =
            adapteros_lora_worker::backend_factory::capabilities::get_available_backends();
        println!("{}", serde_json::to_string_pretty(&backends)?);
        return Ok(());
    }

    if args.detailed {
        adapteros_lora_worker::backend_factory::capabilities::print_backend_status();
    } else {
        // Simple summary
        println!("🔧 AdapterOS Backend Status");
        println!("===========================");

        let backends =
            adapteros_lora_worker::backend_factory::capabilities::get_available_backends();
        let real_backends = backends
            .iter()
            .filter(|b| b.available && !b.stub_only)
            .count();
        let stub_backends = backends
            .iter()
            .filter(|b| b.available && b.stub_only)
            .count();
        let unavailable_backends = backends.iter().filter(|b| !b.available).count();

        println!("✅ Real backends: {}", real_backends);
        println!("⚠️  Stub backends: {}", stub_backends);
        println!("❌ Unavailable: {}", unavailable_backends);
        println!();
        println!("💡 Use --detailed for full report");
        println!("📖 See BACKEND_STATUS.md for implementation details");
    }

    Ok(())
}

