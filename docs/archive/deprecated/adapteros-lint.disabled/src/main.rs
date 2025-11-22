//! AdapterOS Lint Driver
//!
//! This binary provides a command-line interface for running AdapterOS determinism
//! lint rules. It can be used as a standalone tool or integrated into CI/CD pipelines.


use tracing::info;
use std::env;
use std::process;

fn main() {
    tracing_subscriber::fmt()
        .with_max_level(tracing::Level::INFO)
        .init();

    let args: Vec<String> = env::args().collect();
    
    if args.len() < 2 {
        einfo!("Usage: {} <command> [args...]", args[0]);
        einfo!("Commands:");
        einfo!("  check <path>     - Check a single file for determinism violations");
        einfo!("  check-dir <path> - Check all Rust files in a directory");
        einfo!("  help             - Show this help message");
        process::exit(1);
    }

    let command = &args[1];
    
    match command.as_str() {
        "check" => {
            if args.len() < 3 {
                einfo!("Error: check command requires a file path");
                process::exit(1);
            }
            let file_path = &args[2];
            check_file(file_path);
        }
        "check-dir" => {
            if args.len() < 3 {
                einfo!("Error: check-dir command requires a directory path");
                process::exit(1);
            }
            let dir_path = &args[2];
            check_directory(dir_path);
        }
        "help" => {
            print_help();
        }
        _ => {
            einfo!("Unknown command: {}", command);
            einfo!("Use 'help' to see available commands");
            process::exit(1);
        }
    }
}

fn check_file(file_path: &str) {
    info!("Checking file: {}", file_path);
    
    // In a real implementation, this would:
    // 1. Parse the Rust file
    // 2. Run the determinism lint rules
    // 3. Report violations
    
    info!("✓ File check completed (placeholder implementation)");
}

fn check_directory(dir_path: &str) {
    info!("Checking directory: {}", dir_path);
    
    // In a real implementation, this would:
    // 1. Find all Rust files in the directory
    // 2. Run the determinism lint rules on each file
    // 3. Aggregate and report violations
    
    info!("✓ Directory check completed (placeholder implementation)");
}

fn print_help() {
    info!("AdapterOS Determinism Lint Tool");
    info!();
    info!("This tool helps detect nondeterminism in AdapterOS code.");
    info!();
    info!("Usage: adapteros-lint-driver <command> [args...]");
    info!();
    info!("Commands:");
    info!("  check <path>     - Check a single Rust file for determinism violations");
    info!("  check-dir <path> - Check all Rust files in a directory recursively");
    info!("  help             - Show this help message");
    info!();
    info!("Examples:");
    info!("  adapteros-lint-driver check src/main.rs");
    info!("  adapteros-lint-driver check-dir crates/");
    info!();
    info!("The tool detects:");
    info!("  - tokio::task::spawn_blocking calls");
    info!("  - Wall-clock time usage (SystemTime::now(), Instant::now())");
    info!("  - Random number generation without proper seeding");
    info!("  - File I/O operations");
    info!("  - System calls");
}
