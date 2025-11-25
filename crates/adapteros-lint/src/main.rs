//! AdapterOS Lint Driver
//!
//! This binary provides a command-line interface for running AdapterOS architectural
//! lint rules. It can be used as a standalone tool or integrated into CI/CD pipelines.

use adapteros_lint::architectural::{check_directory, check_file, ArchitecturalViolation};
use std::env;
use std::path::Path;
use std::process;

fn main() {
    let args: Vec<String> = env::args().collect();

    if args.len() < 2 {
        eprintln!("Usage: {} <command> [args...]", args[0]);
        eprintln!("Commands:");
        eprintln!("  check <path>     - Check a single file for architectural violations");
        eprintln!("  check-all        - Check all handlers for architectural violations");
        eprintln!("  help             - Show this help message");
        process::exit(1);
    }

    let command = &args[1];

    match command.as_str() {
        "check" => {
            if args.len() < 3 {
                eprintln!("Error: check command requires a file or directory path");
                process::exit(1);
            }
            let path = &args[2];
            let path_obj = Path::new(path);
            if path_obj.is_file() {
                check_file_path(path);
            } else if path_obj.is_dir() {
                check_directory_path(path);
            } else {
                eprintln!("Error: path does not exist: {}", path);
                process::exit(1);
            }
        }
        "check-all" => {
            check_all_handlers();
        }
        "help" => {
            print_help();
        }
        _ => {
            eprintln!("Unknown command: {}", command);
            eprintln!("Use 'help' to see available commands");
            process::exit(1);
        }
    }
}

fn check_file_path(file_path: &str) {
    let path = Path::new(file_path);
    let violations = check_file(path);
    report_violations(&violations, file_path);
    if !violations.is_empty() {
        process::exit(1);
    }
}

fn check_directory_path(dir_path: &str) {
    let path = Path::new(dir_path);
    let violations = check_directory(path);
    report_violations(&violations, dir_path);
    if !violations.is_empty() {
        process::exit(1);
    }
}

fn check_all_handlers() {
    let handlers_path = Path::new("crates/adapteros-server-api/src/handlers");
    if !handlers_path.exists() {
        eprintln!(
            "Error: handlers directory not found: {}",
            handlers_path.display()
        );
        process::exit(1);
    }

    let violations = check_directory(handlers_path);
    report_violations(&violations, "all handlers");
    if !violations.is_empty() {
        process::exit(1);
    }
}

fn report_violations(violations: &[ArchitecturalViolation], context: &str) {
    if violations.is_empty() {
        println!("✓ No architectural violations found in {}", context);
        return;
    }

    eprintln!(
        "Found {} architectural violation(s) in {}:",
        violations.len(),
        context
    );
    eprintln!();

    for violation in violations {
        match violation {
            ArchitecturalViolation::LifecycleManagerBypass {
                file,
                line,
                context,
            } => {
                eprintln!("  [Lifecycle Manager Bypass] {}:{}", file, line);
                eprintln!("    Context: {}", context);
            }
            ArchitecturalViolation::NonTransactionalFallback {
                file,
                line,
                context,
            } => {
                eprintln!("  [Non-Transactional Fallback] {}:{}", file, line);
                eprintln!("    Context: {}", context);
                eprintln!(
                    "    Fix: Use update_adapter_state_tx() instead of update_adapter_state()"
                );
            }
            ArchitecturalViolation::DirectSqlInHandler { file, line, query } => {
                eprintln!("  [Direct SQL in Handler] {}:{}", file, line);
                eprintln!("    Query: {}", query);
                eprintln!("    Fix: Use Db trait method instead");
            }
            ArchitecturalViolation::NonDeterministicSpawn {
                file,
                line,
                context,
            } => {
                eprintln!("  [Non-Deterministic Spawn] {}:{}", file, line);
                eprintln!("    Context: {}", context);
                eprintln!("    Fix: Use spawn_deterministic() instead");
            }
        }
        eprintln!();
    }
}

fn print_help() {
    println!("AdapterOS Architectural Lint Tool");
    println!();
    println!("This tool helps detect architectural violations in AdapterOS code.");
    println!();
    println!("Usage: adapteros-lint <command> [args...]");
    println!();
    println!("Commands:");
    println!("  check <path>     - Check a single file or directory for violations");
    println!("  check-all        - Check all handlers for violations");
    println!("  help             - Show this help message");
    println!();
    println!("Examples:");
    println!("  adapteros-lint check crates/adapteros-server-api/src/handlers.rs");
    println!("  adapteros-lint check crates/adapteros-server-api/src/handlers/");
    println!("  adapteros-lint check-all");
    println!();
    println!("The tool detects:");
    println!("  - Lifecycle manager bypasses (direct DB updates before lifecycle manager)");
    println!("  - Non-transactional fallbacks (should use update_adapter_state_tx)");
    println!("  - Direct SQL queries in handlers (should use Db trait methods)");
    println!("  - Non-deterministic spawns in deterministic contexts");
}
