# AdapterOS CLI Patterns and Conventions

## Overview

The AdapterOS CLI (`aosctl`) is a comprehensive command-line tool located in `crates/adapteros-cli/`. It uses Clap for argument parsing and provides a rich set of commands for managing adapters, training, inference, database operations, and system administration.

## Project Structure

```
crates/adapteros-cli/
├── Cargo.toml           # Dependencies: clap, comfy-table, reqwest, tokio, etc.
├── src/
│   ├── main.rs          # Entry point with Cli struct and Commands enum
│   ├── lib.rs           # Library exports for testing
│   ├── cli.rs           # Alternative CLI struct (secondary)
│   ├── output.rs        # OutputWriter and OutputMode utilities
│   ├── formatting.rs    # format_bytes, format_duration, truncate_id, etc.
│   ├── error_codes.rs   # ECode enum, ErrorCode registry, ExitCode mapping
│   ├── cli_telemetry.rs # Telemetry emission for CLI commands
│   ├── logging.rs       # Logging initialization
│   ├── http_client.rs   # HTTP client with auth token refresh
│   ├── auth_store.rs    # CLI authentication storage
│   ├── validation.rs    # Input validation utilities
│   ├── progress.rs      # Progress indicators
│   └── commands/        # All command implementations (~90+ modules)
│       ├── mod.rs       # Module exports
│       ├── db.rs        # Database commands (migrate, reset, seed-fixtures)
│       ├── train_cli.rs # Training commands (start, status, list)
│       ├── chat.rs      # Interactive chat (interactive, prompt, list, history)
│       ├── serve.rs     # Server startup
│       ├── models.rs    # Model management (seed, list, check-tokenizer)
│       ├── adapter.rs   # Adapter lifecycle
│       ├── stack.rs     # Stack management
│       └── ...          # Many more command modules
└── tests/
    ├── error_handling_tests.rs
    └── output_formatting_tests.rs
```

## Command Structure Pattern

### 1. Main CLI Definition (main.rs)

```rust
#[derive(Parser)]
#[command(name = "aosctl")]
#[command(about = "AdapterOS command-line interface", long_about = None)]
#[command(version)]
struct Cli {
    #[command(subcommand)]
    command: Commands,

    /// Output in JSON format
    #[arg(long, global = true)]
    json: bool,

    /// Suppress non-essential output
    #[arg(long, short = 'q', global = true)]
    quiet: bool,

    /// Enable verbose output
    #[arg(long, short = 'v', global = true)]
    verbose: bool,

    // Global model configuration
    #[arg(long, global = true, env = "AOS_MODEL_PATH")]
    pub model_path: Option<String>,
}
```

### 2. Commands Enum Pattern

```rust
#[derive(Subcommand)]
enum Commands {
    // Subcommand groups use nested enums
    #[command(subcommand, visible_alias = "adapters")]
    Adapter(adapter::AdapterCommand),

    // Simple commands with inline args
    #[command(after_help = "Examples:\n  aosctl serve ...")]
    Serve {
        #[arg(short, long)]
        tenant: String,
        #[arg(long)]
        dry_run: bool,
    },

    // Feature-gated commands
    #[cfg(feature = "scenarios")]
    #[command(subcommand)]
    Scenario(scenario::ScenarioSubcommand),

    // Deprecated commands (hidden)
    #[command(name = "adapter-list", hide = true)]
    AdapterListDeprecated { ... },
}
```

### 3. Command Module Pattern (e.g., db.rs)

```rust
use clap::Subcommand;
use crate::output::OutputWriter;
use anyhow::Result;

#[derive(Debug, Clone, Subcommand)]
pub enum DbCommand {
    /// Run database migrations
    #[command(after_help = r#"Examples:
  aosctl db migrate
  aosctl db migrate --db-path var/custom.db
"#)]
    Migrate {
        #[arg(long)]
        db_path: Option<PathBuf>,
        #[arg(long)]
        verify_only: bool,
    },

    Reset { ... },
    SeedFixtures { ... },
}

/// Handle database commands
pub async fn handle_db_command(cmd: DbCommand, output: &OutputWriter) -> Result<()> {
    match cmd {
        DbCommand::Migrate { db_path, verify_only } => {
            run_migrate(db_path, verify_only, output).await
        }
        DbCommand::Reset { ... } => run_reset(...).await,
        // ...
    }
}

async fn run_migrate(..., output: &OutputWriter) -> Result<()> {
    output.info("Running migrations...");
    // Implementation
    output.success("Migrations complete");
    Ok(())
}
```

## Output Formatting Pattern

### OutputWriter Usage

```rust
use crate::output::OutputWriter;

pub async fn run(output: &OutputWriter) -> Result<()> {
    // Section headers
    output.section("Processing Adapters");

    // Progress messages (suppressed in quiet mode)
    output.progress("Loading configuration...");
    output.progress_done(true);  // Shows ✓ Done

    // Key-value pairs
    output.kv("Tenant", &tenant_id);
    output.kv("Status", "active");

    // Blank lines for spacing
    output.blank();

    // Success/error/warning messages
    output.success("Operation completed");
    output.error("Something failed");
    output.warning("Non-critical issue");

    // Info messages
    output.info("Additional information");

    // Results (always shown unless JSON mode)
    output.result("Final output");

    // JSON output (when --json flag is set)
    if output.is_json() {
        output.json(&data)?;
    }

    Ok(())
}
```

### OutputMode

```rust
pub enum OutputMode {
    Text,   // Normal human-readable output
    Json,   // Machine-readable JSON
    Quiet,  // Minimal output (CI mode)
}

// Created from CLI flags
let mode = OutputMode::from_flags(cli.json, cli.quiet);
let output = OutputWriter::new(mode, cli.verbose);
```

## Error Handling Pattern

### Error Codes (error_codes.rs)

```rust
// Define typed error codes
define_ecodes! {
    "Crypto/Signing" => [E1001, E1002, E1003, E1004],
    "Policy/Determinism" => [E2001, E2002, ...],
    "CLI/Config" => [E8001, E8002, ...],
    // Categories: E1xxx-E9xxx
}

// Error code with recovery information
pub struct ErrorCode {
    pub ecode: ECode,
    pub code: &'static str,      // "E3001"
    pub category: &'static str,  // "Kernels/Build/Manifest"
    pub title: &'static str,
    pub cause: &'static str,
    pub fix: &'static str,       // Multi-line recovery steps
    pub related_docs: &'static [&'static str],
}

// Usage: aosctl explain E3001
```

### Exit Codes

```rust
#[repr(u8)]
pub enum ExitCode {
    Success = 0,
    GeneralError = 1,
    Config = 10,
    Database = 20,
    Network = 30,
    Crypto = 40,
    PolicyViolation = 50,
    Validation = 60,
    // ... mapped from AosError variants
}
```

## Adding a New Command

### Step 1: Create Command Module

```rust
// src/commands/mycommand.rs
use clap::{Args, Subcommand};
use crate::output::OutputWriter;
use anyhow::Result;

#[derive(Debug, Clone, Subcommand)]
pub enum MyCommand {
    /// Do something
    #[command(after_help = "Examples:\n  aosctl mycommand do --flag value")]
    Do(DoArgs),

    /// List things
    List {
        #[arg(long)]
        json: bool,
    },
}

#[derive(Debug, Clone, Args)]
pub struct DoArgs {
    /// Required argument
    pub target: String,

    /// Optional flag with default
    #[arg(long, default_value = "default")]
    pub flag: String,
}

pub async fn handle_mycommand(cmd: MyCommand, output: &OutputWriter) -> Result<()> {
    match cmd {
        MyCommand::Do(args) => run_do(args, output).await,
        MyCommand::List { json } => run_list(json, output).await,
    }
}

async fn run_do(args: DoArgs, output: &OutputWriter) -> Result<()> {
    output.section("Doing something");
    output.kv("Target", &args.target);
    // ... implementation
    output.success("Done!");
    Ok(())
}
```

### Step 2: Register in mod.rs

```rust
// src/commands/mod.rs
pub mod mycommand;
```

### Step 3: Add to Commands Enum (main.rs)

```rust
#[derive(Subcommand)]
enum Commands {
    // ...existing commands...

    /// My new command
    #[command(subcommand)]
    Mycommand(mycommand::MyCommand),
}
```

### Step 4: Handle in execute_command()

```rust
async fn execute_command(command: &Commands, cli: &Cli, output: &OutputWriter) -> Result<()> {
    match command {
        // ...existing handlers...
        Commands::Mycommand(cmd) => {
            mycommand::handle_mycommand(cmd.clone(), output).await?;
        }
    }
    Ok(())
}
```

### Step 5: Add Telemetry Name

```rust
fn get_command_name(command: &Commands) -> String {
    match command {
        // ...
        Commands::Mycommand(_) => "mycommand",
        // ...
    }.to_string()
}
```

## Key Conventions

### 1. Argument Naming
- Use `--long-name` for optional flags
- Use `--db-path` pattern for path overrides
- Support env vars: `#[arg(long, env = "AOS_VAR_NAME")]`
- Add `after_help` with examples for complex commands

### 2. Output Patterns
- Always accept `&OutputWriter` parameter
- Use `output.is_json()` to check for JSON mode
- Emit telemetry via `cli_telemetry::emit_cli_command()`
- Log with `tracing::info!()` for debugging

### 3. Async by Default
- Most handlers are `async fn`
- Use `tokio::main` in main.rs
- HTTP calls use `reqwest` with auth token refresh

### 4. Testing
- Use `OutputWriter::with_sink()` to capture output in tests
- Use mock HTTP servers for API tests
- Test error code formatting and uniqueness

### 5. Feature Flags
```toml
[features]
default = ["replay", "trace", "orchestrator"]
multi-backend = ["dep:adapteros-lora-mlx-ffi"]
tui = ["dep:adapteros-tui"]
scenarios = []
```

## Common Imports for Command Modules

```rust
use anyhow::{bail, Context, Result};
use clap::{Args, Subcommand};
use std::path::PathBuf;
use tracing::{info, warn, error};

use crate::output::OutputWriter;
use crate::http_client::send_with_refresh_from_store;
use adapteros_db::Db;
use adapteros_api_types::*;
```

## Table Formatting

```rust
use crate::output::create_styled_table;

let mut table = create_styled_table();
table.set_header(vec!["ID", "Name", "Status"]);
table.add_row(vec!["id-1", "Adapter 1", "active"]);
output.table(&table, Some(&json_data))?;
```
