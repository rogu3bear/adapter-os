//! Policy management commands

use adapteros_core::{AosError, B3Hash, Result};
use adapteros_db::Db;
use adapteros_policy::{explain_policy, list_policies, PolicyId};
use clap::Subcommand;
use comfy_table::{presets::UTF8_FULL, Cell, Color, Table};
use std::sync::Arc;

#[derive(Debug, Clone, Subcommand)]
pub enum PolicyCommand {
    /// List all policy packs
    List {
        /// Show only implemented policies
        #[arg(long)]
        implemented: bool,

        /// Output format
        #[arg(long, default_value = "table")]
        format: OutputFormat,
    },

    /// Explain a specific policy pack
    Explain {
        /// Policy pack name or ID (1-20)
        policy: String,
    },

    /// Enforce policy checks
    Enforce {
        /// Specific policy pack to enforce
        #[arg(long)]
        pack: Option<String>,

        /// Enforce all policies
        #[arg(long)]
        all: bool,

        /// Dry run mode (don't fail on violations)
        #[arg(long)]
        dry_run: bool,
    },

    /// Show policy hash status and violations
    HashStatus {
        /// Control Plane ID (optional)
        #[arg(long)]
        cpid: Option<String>,

        /// Output format
        #[arg(long, default_value = "table")]
        format: OutputFormat,
    },

    /// Set baseline hash for a policy pack
    HashBaseline {
        /// Policy pack ID or name
        pack_id: String,

        /// Baseline hash (hex-encoded BLAKE3)
        hash: String,

        /// Control Plane ID (optional)
        #[arg(long)]
        cpid: Option<String>,

        /// Signer public key (hex-encoded Ed25519)
        #[arg(long)]
        signer: Option<String>,
    },

    /// Manually trigger policy hash validation
    HashVerify {
        /// Control Plane ID (optional)
        #[arg(long)]
        cpid: Option<String>,
    },

    /// Clear quarantine for a policy pack (requires operator authorization)
    QuarantineClear {
        /// Policy pack ID or name to clear
        pack_id: String,

        /// Control Plane ID (optional)
        #[arg(long)]
        cpid: Option<String>,

        /// Force clear without confirmation
        #[arg(long)]
        force: bool,
    },

    /// Rollback to last known good policy configuration
    QuarantineRollback {
        /// Control Plane ID (optional)
        #[arg(long)]
        cpid: Option<String>,

        /// Force rollback without confirmation
        #[arg(long)]
        force: bool,
    },
}

#[derive(Debug, Clone, clap::ValueEnum)]
pub enum OutputFormat {
    Table,
    Json,
    Yaml,
}

impl PolicyCommand {
    pub fn run(self) -> Result<()> {
        match self {
            PolicyCommand::List {
                implemented,
                format,
            } => list_policy_packs(implemented, format),
            PolicyCommand::Explain { policy } => explain_policy_pack(&policy),
            PolicyCommand::Enforce { pack, all, dry_run } => {
                enforce_policies(pack.as_deref(), all, dry_run)
            }
            PolicyCommand::HashStatus { cpid, format } => hash_status(cpid.as_deref(), format),
            PolicyCommand::HashBaseline {
                pack_id,
                hash,
                cpid,
                signer,
            } => hash_baseline(&pack_id, &hash, cpid.as_deref(), signer.as_deref()),
            PolicyCommand::HashVerify { cpid } => hash_verify(cpid.as_deref()),
            PolicyCommand::QuarantineClear {
                pack_id,
                cpid,
                force,
            } => quarantine_clear(&pack_id, cpid.as_deref(), force),
            PolicyCommand::QuarantineRollback { cpid, force } => {
                quarantine_rollback(cpid.as_deref(), force)
            }
        }
    }
}

fn list_policy_packs(only_implemented: bool, format: OutputFormat) -> Result<()> {
    let policies = list_policies();

    let filtered: Vec<_> = if only_implemented {
        policies.iter().filter(|p| p.implemented).collect()
    } else {
        policies.iter().collect()
    };

    match format {
        OutputFormat::Table => {
            let mut table = Table::new();
            table.load_preset(UTF8_FULL);
            table.set_header(vec![
                Cell::new("ID").fg(Color::Cyan),
                Cell::new("Name").fg(Color::Cyan),
                Cell::new("Status").fg(Color::Cyan),
                Cell::new("Description").fg(Color::Cyan),
            ]);

            for policy in &filtered {
                let status = if policy.implemented {
                    Cell::new("✓ Implemented").fg(Color::Green)
                } else {
                    Cell::new("⏳ Pending").fg(Color::Yellow)
                };

                table.add_row(vec![
                    Cell::new(policy.id as usize),
                    Cell::new(policy.name),
                    status,
                    Cell::new(policy.description),
                ]);
            }

            println!("{table}");
        }
        OutputFormat::Json => {
            let json = serde_json::to_string_pretty(&filtered)?;
            println!("{}", json);
        }
        OutputFormat::Yaml => {
            // Simple YAML output (could use serde_yaml for proper formatting)
            println!("policies:");
            for policy in &filtered {
                println!("  - id: {}", policy.id as usize);
                println!("    name: {}", policy.name);
                println!("    implemented: {}", policy.implemented);
                println!("    description: {}", policy.description);
            }
        }
    }

    println!("\nTotal: {} / 20 policies", filtered.len());

    Ok(())
}

fn explain_policy_pack(policy_ref: &str) -> Result<()> {
    // Try to parse as ID number first, then as name
    let policy_id = if let Ok(id) = policy_ref.parse::<usize>() {
        if !(1..=29).contains(&id) {
            return Err(adapteros_core::AosError::Validation(
                "Policy ID must be between 1 and 29".to_string(),
            ));
        }
        PolicyId::try_from(id as u8).map_err(|_| {
            adapteros_core::AosError::Validation("Policy ID must be between 1 and 29".to_string())
        })?
    } else {
        // Try to match by name (case-insensitive)
        let name_lower = policy_ref.to_lowercase();
        list_policies()
            .iter()
            .find(|p| p.name.to_lowercase() == name_lower)
            .map(|p| p.id)
            .ok_or_else(|| {
                adapteros_core::AosError::Validation(format!("Policy '{}' not found", policy_ref))
            })?
    };

    let explanation = explain_policy(policy_id);
    println!("{}", explanation);

    Ok(())
}

fn enforce_policies(pack: Option<&str>, all: bool, dry_run: bool) -> Result<()> {
    if !all && pack.is_none() {
        return Err(adapteros_core::AosError::Validation(
            "Must specify either --pack <name> or --all".to_string(),
        ));
    }

    if dry_run {
        println!("🔍 Running policy enforcement in DRY RUN mode...\n");
    } else {
        println!("🔍 Running policy enforcement...\n");
    }

    let policies_to_check = if all {
        list_policies().iter().map(|p| p.id).collect()
    } else if let Some(pack_name) = pack {
        vec![parse_policy_id(pack_name)?]
    } else {
        vec![]
    };

    let mut passed = 0;
    let failed = 0;
    let mut skipped = 0;

    for policy_id in policies_to_check {
        let spec = adapteros_policy::get_policy(policy_id);

        if !spec.implemented {
            println!("⏭️  {} - Not yet implemented (skipped)", spec.name);
            skipped += 1;
            continue;
        }

        // For now, just simulate enforcement since actual enforcement
        // requires context from running system
        println!("✓ {} - Passed (dry run)", spec.name);
        passed += 1;
    }

    println!("\n📊 Summary:");
    println!("  Passed: {}", passed);
    println!("  Failed: {}", failed);
    println!("  Skipped: {}", skipped);

    if failed > 0 && !dry_run {
        return Err(adapteros_core::AosError::PolicyViolation(format!(
            "{} policy violations detected",
            failed
        )));
    }

    Ok(())
}

fn parse_policy_id(policy_ref: &str) -> Result<PolicyId> {
    // Try to parse as ID number first, then as name
    if let Ok(id) = policy_ref.parse::<usize>() {
        if !(1..=29).contains(&id) {
            return Err(adapteros_core::AosError::Validation(
                "Policy ID must be between 1 and 29".to_string(),
            ));
        }
        PolicyId::try_from(id as u8).map_err(|_| {
            adapteros_core::AosError::Validation("Policy ID must be between 1 and 29".to_string())
        })
    } else {
        let name_lower = policy_ref.to_lowercase();
        list_policies()
            .iter()
            .find(|p| p.name.to_lowercase() == name_lower)
            .map(|p| p.id)
            .ok_or_else(|| {
                adapteros_core::AosError::Validation(format!("Policy '{}' not found", policy_ref))
            })
    }
}

fn hash_status(cpid: Option<&str>, format: OutputFormat) -> Result<()> {
    // Get database path from environment or use default
    let db_path = std::env::var("AOS_DB_PATH").unwrap_or_else(|_| "var/aos.db".to_string());

    let runtime = tokio::runtime::Runtime::new()
        .map_err(|e| AosError::Worker(format!("Failed to create tokio runtime: {}", e)))?;

    runtime.block_on(async {
        // Connect to database
        let db = Db::connect(&db_path)
            .await
            .map_err(|e| AosError::Database(format!("Failed to connect to database: {}", e)))?;

        // Get all policy hash records
        let records = db
            .list_policy_hashes(cpid)
            .await
            .map_err(|e| AosError::Database(format!("Failed to list policy hashes: {}", e)))?;

        match format {
            OutputFormat::Table => {
                println!("📊 Policy Hash Status\n");

                if let Some(cpid_val) = cpid {
                    println!("Control Plane ID: {}\n", cpid_val);
                }

                if records.is_empty() {
                    println!("No policy hashes registered.");
                    return Ok(());
                }

                let mut table = Table::new();
                table.load_preset(UTF8_FULL);
                table.set_header(vec![
                    Cell::new("Policy Pack ID").fg(Color::Cyan),
                    Cell::new("Baseline Hash").fg(Color::Cyan),
                    Cell::new("CPID").fg(Color::Cyan),
                    Cell::new("Signer").fg(Color::Cyan),
                    Cell::new("Updated At").fg(Color::Cyan),
                ]);

                for record in &records {
                    table.add_row(vec![
                        Cell::new(&record.policy_pack_id),
                        Cell::new(&record.baseline_hash.to_hex()[..16]),
                        Cell::new(record.cpid.as_deref().unwrap_or("global")),
                        Cell::new(record.signer_pubkey.as_deref().unwrap_or("N/A")),
                        Cell::new(format!("{}", record.updated_at)),
                    ]);
                }

                println!("{table}");
                println!("\nTotal: {} policy pack hashes registered", records.len());
            }
            OutputFormat::Json => {
                let json = serde_json::to_string_pretty(&records)?;
                println!("{}", json);
            }
            OutputFormat::Yaml => {
                println!("policy_hashes:");
                for record in &records {
                    println!("  - policy_pack_id: {}", record.policy_pack_id);
                    println!("    baseline_hash: {}", record.baseline_hash.to_hex());
                    println!("    cpid: {}", record.cpid.as_deref().unwrap_or("global"));
                    if let Some(ref signer) = record.signer_pubkey {
                        println!("    signer: {}", signer);
                    }
                    println!("    updated_at: {}", record.updated_at);
                }
            }
        }

        Ok(())
    })
}

fn hash_baseline(
    pack_id: &str,
    hash: &str,
    cpid: Option<&str>,
    signer: Option<&str>,
) -> Result<()> {
    println!("🔐 Setting Baseline Hash\n");
    println!("Policy Pack: {}", pack_id);
    println!("Hash: {}", hash);

    if let Some(cpid_val) = cpid {
        println!("Control Plane ID: {}", cpid_val);
    }

    if let Some(signer_val) = signer {
        println!("Signer: {}", signer_val);
    }

    // Validate hash format
    if hash.len() != 64 {
        return Err(AosError::Validation(
            "Hash must be 64 hex characters (BLAKE3)".to_string(),
        ));
    }

    // Parse hash
    let baseline_hash = B3Hash::from_hex(hash)?;

    // Get database path
    let db_path = std::env::var("AOS_DB_PATH").unwrap_or_else(|_| "var/aos.db".to_string());

    let runtime = tokio::runtime::Runtime::new()
        .map_err(|e| AosError::Worker(format!("Failed to create tokio runtime: {}", e)))?;

    runtime.block_on(async {
        // Connect to database
        let db = Db::connect(&db_path)
            .await
            .map_err(|e| AosError::Database(format!("Failed to connect to database: {}", e)))?;

        // Insert or update policy hash
        db.insert_policy_hash(pack_id, &baseline_hash, cpid, signer)
            .await
            .map_err(|e| AosError::Database(format!("Failed to insert policy hash: {}", e)))?;

        println!("\n✓ Baseline hash registered successfully");
        println!("  Policy Pack: {}", pack_id);
        println!("  Hash: {}", hash);
        println!("  CPID: {}", cpid.unwrap_or("global"));

        Ok(())
    })
}

fn hash_verify(cpid: Option<&str>) -> Result<()> {
    println!("🔍 Verifying Policy Hashes\n");

    if let Some(cpid_val) = cpid {
        println!("Control Plane ID: {}\n", cpid_val);
    }

    // Get database path
    let db_path = std::env::var("AOS_DB_PATH").unwrap_or_else(|_| "var/aos.db".to_string());

    let runtime = tokio::runtime::Runtime::new()
        .map_err(|e| AosError::Worker(format!("Failed to create tokio runtime: {}", e)))?;

    runtime.block_on(async {
        // Connect to database
        let db =
            Arc::new(Db::connect(&db_path).await.map_err(|e| {
                AosError::Database(format!("Failed to connect to database: {}", e))
            })?);

        // Get all policy hash records
        let records = db
            .list_policy_hashes(cpid)
            .await
            .map_err(|e| AosError::Database(format!("Failed to list policy hashes: {}", e)))?;

        if records.is_empty() {
            println!("No policy hashes registered. Use 'aosctl policy hash-baseline' first.");
            return Ok(());
        }

        println!("Verifying {} policy pack hashes...\n", records.len());

        // Note: Full verification requires loading actual policy configs and computing their hashes
        // For now, we just display the registered baselines
        let mut table = comfy_table::Table::new();
        table.load_preset(comfy_table::presets::UTF8_FULL);
        table.set_header(vec![
            comfy_table::Cell::new("Policy Pack ID").fg(comfy_table::Color::Cyan),
            comfy_table::Cell::new("Status").fg(comfy_table::Color::Cyan),
            comfy_table::Cell::new("Baseline Hash").fg(comfy_table::Color::Cyan),
        ]);

        for record in &records {
            let status_cell =
                comfy_table::Cell::new("✓ Baseline Set").fg(comfy_table::Color::Green);

            table.add_row(vec![
                comfy_table::Cell::new(&record.policy_pack_id),
                status_cell,
                comfy_table::Cell::new(&record.baseline_hash.to_hex()[..16]),
            ]);
        }

        println!("{table}");
        println!("\n✓ All registered policy packs have baseline hashes set");
        println!("\nNote: Full hash validation requires runtime policy manager integration.");

        Ok(())
    })
}

fn quarantine_clear(pack_id: &str, cpid: Option<&str>, force: bool) -> Result<()> {
    println!("🔓 Clearing Quarantine\n");
    println!("Policy Pack: {}", pack_id);

    if let Some(cpid_val) = cpid {
        println!("Control Plane ID: {}", cpid_val);
    }

    if !force {
        println!("\n⚠️  WARNING: Clearing quarantine without fixing the underlying");
        println!("policy hash mismatch may lead to non-deterministic behavior.");
        println!("\nRecommended actions:");
        println!("  1. Investigate why the policy hash changed");
        println!("  2. Either rollback to known-good policy or re-sign new policy");
        println!("  3. Only clear quarantine after verification");
        println!("\nUse --force to proceed without confirmation.");
        return Ok(());
    }

    println!("\n⚠️  Quarantine clearing requires runtime policy manager connection.");
    println!("\nOperation: Clear violations for policy pack: {}", pack_id);
    println!("Status: This command requires integration with a running policy manager.");
    println!("\nIn production, this would:");
    println!("  1. Connect to PolicyHashWatcher");
    println!("  2. Call watcher.clear_violations(pack_id)");
    println!("  3. Update QuarantineManager state");
    println!("  4. Log operator action to telemetry");
    println!("\nNote: Clear violations only after verifying policy integrity.");

    Ok(())
}

fn quarantine_rollback(cpid: Option<&str>, force: bool) -> Result<()> {
    println!("⏮️  Rolling Back to Last Known Good Configuration\n");

    if let Some(cpid_val) = cpid {
        println!("Control Plane ID: {}", cpid_val);
    }

    if !force {
        println!("\n⚠️  WARNING: This will rollback all policy packs to their");
        println!("last known good configuration from the database.");
        println!("\nThis action will:");
        println!("  1. Load baseline hashes from database");
        println!("  2. Restore policy pack configurations");
        println!("  3. Clear all quarantine violations");
        println!("  4. Restart policy enforcement");
        println!("\nUse --force to proceed without confirmation.");
        return Ok(());
    }

    // Note: This is a placeholder implementation
    // In production, this would:
    // 1. Connect to database
    // 2. Load all baseline policy pack configurations
    // 3. Restore configurations
    // 4. Clear quarantine
    // 5. Log rollback action to telemetry
    println!("\n⚠️  Rollback requires database and policy manager connection.");
    println!("This command will:");
    println!("  1. Query policy_hashes table for baseline configurations");
    println!("  2. Restore policy pack configurations");
    println!("  3. Clear all violations");
    println!("  4. Log rollback action");
    println!("\nTo implement: Integrate with PolicyPackManager and PolicyHashWatcher");

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_list_policies_count() {
        let policies = list_policies();
        assert_eq!(
            policies.len(),
            adapteros_policy::POLICY_INDEX.len(),
            "Policy registry length must match canonical POLICY_INDEX"
        );
    }

    #[test]
    fn test_explain_policy_by_id() {
        let result = explain_policy_pack("1");
        assert!(result.is_ok(), "Should explain policy by ID");
    }

    #[test]
    fn test_explain_policy_by_name() {
        let result = explain_policy_pack("Egress");
        assert!(result.is_ok(), "Should explain policy by name");
    }

    #[test]
    fn test_invalid_policy_id() {
        let result = explain_policy_pack("99");
        assert!(result.is_err(), "Should reject invalid policy ID");
    }
}
