//! CodeGraph CLI binary
//!
//! Command-line interface for building and analyzing code graphs

use adapteros_codegraph::{CodeGraph, DbConfig};
use clap::Parser;
use std::path::PathBuf;

#[derive(Parser)]
#[clap(name = "codegraph", about = "CodeGraph analysis tool")]
struct Args {
    /// Input directory containing Rust source code
    #[clap(short, long)]
    input: PathBuf,

    /// Output database path
    #[clap(short, long)]
    output: PathBuf,

    /// Enable verbose output
    #[clap(short, long)]
    verbose: bool,
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args = Args::parse();

    if args.verbose {
        env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("info")).init();
    }

    println!("🔍 Building CodeGraph from: {}", args.input.display());

    // Create database configuration
    let db_config = DbConfig {
        path: args.output.to_string_lossy().to_string(),
        pool_size: 10,
        enable_wal: true,
    };

    // Build CodeGraph from source directory
    let codegraph = CodeGraph::from_directory(&args.input, Some(db_config)).await?;

    // Save to database
    codegraph.save_to_db(&args.output).await?;

    println!("✅ CodeGraph built successfully");
    println!("   Symbols: {}", codegraph.symbols.len());
    println!("   Edges: {}", codegraph.call_graph.edges.len());
    println!("   Content hash: {}", codegraph.content_hash.to_short_hex());
    println!("   Database: {}", args.output.display());

    Ok(())
}
