//! Deployment commands for aosctl.
//!
//! Currently provides:
//! - `aosctl deploy adapters` – deploy adapter directories / .aos / .safetensors
//!
//! Behavior is based on the legacy `scripts/deploy_adapters.sh` helper
//! while integrating with `OutputWriter` and JSON/quiet modes.
//!
//! [source: scripts/deploy_adapters.sh L1-L120]

use crate::output::OutputWriter;
use anyhow::{Context, Result};
use clap::{Args, Subcommand};
use serde::Serialize;
use std::fs;
use std::path::{Path, PathBuf};

/// Top-level deploy command.
#[derive(Debug, Args, Clone)]
pub struct DeployCommand {
    #[command(subcommand)]
    pub subcommand: DeploySubcommand,
}

/// Deploy subcommands.
#[derive(Debug, Subcommand, Clone)]
pub enum DeploySubcommand {
    /// Deploy adapter artifacts (directories, .aos, .safetensors)
    Adapters(DeployAdaptersArgs),
}

/// Arguments for `aosctl deploy adapters`.
#[derive(Debug, Args, Clone)]
pub struct DeployAdaptersArgs {
    /// Adapter paths (directory, .aos, or .safetensors)
    #[arg(long, required = true)]
    pub path: Vec<PathBuf>,

    /// Target adapters directory
    #[arg(long, default_value = "/opt/adapteros/adapters")]
    pub adapters_dir: PathBuf,

    /// Backup existing adapters before overwrite
    #[arg(long)]
    pub backup_existing: bool,

    /// Dry run – report actions without modifying disk
    #[arg(long)]
    pub dry_run: bool,
}

/// JSON result for deploy adapters.
#[derive(Debug, Serialize)]
struct DeployAdaptersResult {
    target_dir: String,
    dry_run: bool,
    backup_existing: bool,
    items: Vec<DeployedItem>,
}

#[derive(Debug, Serialize)]
struct DeployedItem {
    source: String,
    adapter_name: String,
    kind: String,
    backed_up: bool,
    registered: bool,
}

/// Dispatch deploy commands.
pub async fn run(cmd: DeployCommand, output: &OutputWriter) -> Result<()> {
    match cmd.subcommand {
        DeploySubcommand::Adapters(args) => deploy_adapters(args, output).await,
    }
}

async fn deploy_adapters(args: DeployAdaptersArgs, output: &OutputWriter) -> Result<()> {
    let mut items = Vec::new();

    // Ensure target directory exists (unless dry-run)
    if !args.dry_run {
        fs::create_dir_all(&args.adapters_dir)
            .with_context(|| format!("creating adapters dir {}", args.adapters_dir.display()))?;
    }

    // Optional backup root: /opt/adapteros/adapters.backup.YYYYMMDD_HHMMSS
    let backup_root = if args.backup_existing {
        let ts = chrono::Local::now().format("%Y%m%d_%H%M%S");
        Some(PathBuf::from(format!(
            "{}.backup.{}",
            args.adapters_dir.display(),
            ts
        )))
    } else {
        None
    };

    if let Some(root) = &backup_root {
        if !args.dry_run {
            fs::create_dir_all(root)
                .with_context(|| format!("creating backup dir {}", root.display()))?;
        }
    }

    if output.is_verbose() && !output.is_json() {
        output.section("Deploying adapters");
        output.kv("Target dir", &args.adapters_dir.display().to_string());
        output.kv("Dry run", &args.dry_run.to_string());
        output.kv("Backup existing", &args.backup_existing.to_string());
    }

    for path in &args.path {
        if !path.exists() {
            output.warning(format!("Adapter path not found: {}", path.display()));
            continue;
        }

        if path.is_dir() {
            let item = deploy_directory_adapter(path, &args, &backup_root, output).await?;
            items.push(item);
        } else if path
            .extension()
            .and_then(|e| e.to_str())
            .map(|e| e.eq_ignore_ascii_case("aos"))
            .unwrap_or(false)
        {
            let item = deploy_aos_adapter(path, &args, &backup_root, output).await?;
            items.push(item);
        } else {
            // Treat as weights file – deploy its parent directory
            if let Some(dir) = path.parent() {
                let item = deploy_directory_adapter(dir, &args, &backup_root, output).await?;
                items.push(item);
            } else {
                output.warning(format!(
                    "Cannot determine adapter directory for {}",
                    path.display()
                ));
            }
        }
    }

    if output.is_json() {
        let result = DeployAdaptersResult {
            target_dir: args.adapters_dir.display().to_string(),
            dry_run: args.dry_run,
            backup_existing: args.backup_existing,
            items,
        };
        output.json(&result)?;
    } else if !output.is_quiet() {
        if args.dry_run {
            output.success("Dry run complete – no changes made");
        } else {
            output.success("Deployment complete");
        }
        if let Some(root) = &backup_root {
            output.kv("Backup dir", &root.display().to_string());
        }
    }

    Ok(())
}

async fn deploy_directory_adapter(
    adapter_dir: &Path,
    args: &DeployAdaptersArgs,
    backup_root: &Option<PathBuf>,
    output: &OutputWriter,
) -> Result<DeployedItem> {
    let adapter_name = adapter_dir
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or("unknown")
        .to_string();

    if output.is_verbose() && !output.is_json() {
        output.info(format!("Deploying directory adapter: {}", adapter_name));
    }

    let target_dir = args.adapters_dir.join(&adapter_name);
    let mut backed_up = false;

    if target_dir.exists() && args.backup_existing {
        if let Some(root) = backup_root {
            let backup_path = root.join(&adapter_name);
            if args.dry_run {
                output.verbose(format!(
                    "[dry-run] would backup existing adapter {} -> {}",
                    target_dir.display(),
                    backup_path.display()
                ));
            } else {
                fs::create_dir_all(root)
                    .with_context(|| format!("creating backup dir {}", root.display()))?;
                fs::remove_dir_all(&backup_path).ok();
                fs::create_dir_all(backup_path.parent().unwrap_or_else(|| Path::new(".")))?;
                fs_extra::dir::copy(
                    &target_dir,
                    &backup_path.parent().unwrap_or(root),
                    &fs_extra::dir::CopyOptions::new().content_only(false),
                )
                .with_context(|| {
                    format!(
                        "backing up existing adapter {} -> {}",
                        target_dir.display(),
                        backup_path.display()
                    )
                })?;
            }
            backed_up = true;
        }
    }

    if args.dry_run {
        output.verbose(format!(
            "[dry-run] would copy dir {} -> {}",
            adapter_dir.display(),
            target_dir.display()
        ));
    } else {
        if target_dir.exists() {
            fs::remove_dir_all(&target_dir).ok();
        }
        fs::create_dir_all(target_dir.parent().unwrap_or_else(|| &args.adapters_dir))?;
        fs_extra::dir::copy(
            adapter_dir,
            &args.adapters_dir,
            &fs_extra::dir::CopyOptions::new().content_only(false),
        )
        .with_context(|| {
            format!(
                "copying adapter dir {} -> {}",
                adapter_dir.display(),
                args.adapters_dir.display()
            )
        })?;
    }

    // Register via HTTP API (aosctl adapters register) using existing implementation
    let mut registered = false;
    if !args.dry_run {
        let register_args = crate::commands::adapters::RegisterArgs {
            path: target_dir.clone(),
            adapter_id: Some(adapter_name.clone()),
            name: None,
            rank: None,
            tier: None,
            base_url: "http://127.0.0.1:8080/api".to_string(),
        };
        let reg_output = OutputWriter::new(output.mode(), output.is_verbose());
        if crate::commands::adapters::run(
            crate::commands::adapters::AdaptersArgs {
                cmd: crate::commands::adapters::AdaptersCmd::Register(register_args),
            },
            &reg_output,
        )
        .await
        .is_ok()
        {
            registered = true;
        } else {
            output.warning(format!(
                "Adapter registration failed for {} (HTTP API)",
                adapter_name
            ));
        }
    }

    Ok(DeployedItem {
        source: adapter_dir.display().to_string(),
        adapter_name,
        kind: "directory".to_string(),
        backed_up,
        registered,
    })
}

async fn deploy_aos_adapter(
    aos_file: &Path,
    args: &DeployAdaptersArgs,
    backup_root: &Option<PathBuf>,
    output: &OutputWriter,
) -> Result<DeployedItem> {
    let adapter_name = aos_file
        .file_stem()
        .and_then(|n| n.to_str())
        .unwrap_or("unknown")
        .to_string();

    if output.is_verbose() && !output.is_json() {
        output.info(format!("Deploying .aos adapter: {}", adapter_name));
    }

    // Verify .aos file
    if args.dry_run {
        output.verbose(format!(
            "[dry-run] would verify .aos file {}",
            aos_file.display()
        ));
    } else {
        let verify_args = crate::commands::aos::VerifyArgs {
            path: aos_file.to_path_buf(),
            format: "text".to_string(),
        };
        let verify_output = OutputWriter::new(output.mode(), output.is_verbose());
        crate::commands::aos::verify_aos(verify_args, &verify_output).await?;
    }

    let target_path = args.adapters_dir.join(format!("{}.aos", adapter_name));
    let mut backed_up = false;

    if target_path.exists() && args.backup_existing {
        if let Some(root) = backup_root {
            let backup_path = root.join(format!("{}.aos", adapter_name));
            if args.dry_run {
                output.verbose(format!(
                    "[dry-run] would backup existing .aos {} -> {}",
                    target_path.display(),
                    backup_path.display()
                ));
            } else {
                fs::create_dir_all(backup_path.parent().unwrap_or_else(|| root.as_path()))?;
                fs::copy(&target_path, &backup_path).with_context(|| {
                    format!(
                        "backing up existing .aos {} -> {}",
                        target_path.display(),
                        backup_path.display()
                    )
                })?;
            }
            backed_up = true;
        }
    }

    if args.dry_run {
        output.verbose(format!(
            "[dry-run] would copy .aos {} -> {}",
            aos_file.display(),
            target_path.display()
        ));
    } else {
        fs::create_dir_all(target_path.parent().unwrap_or_else(|| &args.adapters_dir))?;
        fs::copy(aos_file, &target_path).with_context(|| {
            format!(
                "copying .aos file {} -> {}",
                aos_file.display(),
                target_path.display()
            )
        })?;
    }

    // Load into registry via existing aosctl aos load
    let mut registered = false;
    if !args.dry_run {
        let load_args = crate::commands::aos::LoadArgs {
            path: target_path.clone(),
            adapter_id: None,
            base_url: "http://127.0.0.1:8080/api".to_string(),
        };
        let load_output = OutputWriter::new(output.mode(), output.is_verbose());
        if crate::commands::aos::load_aos(load_args, &load_output)
            .await
            .is_ok()
        {
            registered = true;
        } else {
            output.warning(format!(
                "Failed to load .aos adapter {} into registry",
                adapter_name
            ));
        }
    }

    Ok(DeployedItem {
        source: aos_file.display().to_string(),
        adapter_name,
        kind: "aos".to_string(),
        backed_up,
        registered,
    })
}
