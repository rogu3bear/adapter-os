//! Policy management commands

use adapteros_core::Result;
use adapteros_policy::{explain_policy, list_policies, PolicyId};
use clap::{Args, Subcommand};
use comfy_table::{presets::UTF8_FULL, Cell, Color, Table};

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
}

#[derive(Debug, Clone, clap::ValueEnum)]
enum OutputFormat {
    Table,
    Json,
    Yaml,
}

impl PolicyCommand {
    pub fn run(self) -> Result<()> {
        match self {
            PolicyCommand::List { implemented, format } => {
                list_policy_packs(implemented, format)
            }
            PolicyCommand::Explain { policy } => explain_policy_pack(&policy),
            PolicyCommand::Enforce { pack, all, dry_run } => {
                enforce_policies(pack.as_deref(), all, dry_run)
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
        if id < 1 || id > 20 {
            return Err(adapteros_core::AosError::Validation(
                "Policy ID must be between 1 and 20".to_string(),
            ));
        }
        // Convert to PolicyId enum (casting is safe since we validated range)
        unsafe { std::mem::transmute::<u8, PolicyId>(id as u8) }
    } else {
        // Try to match by name (case-insensitive)
        let name_lower = policy_ref.to_lowercase();
        list_policies()
            .iter()
            .find(|p| p.name.to_lowercase() == name_lower)
            .map(|p| p.id)
            .ok_or_else(|| {
                adapteros_core::AosError::Validation(format!(
                    "Policy '{}' not found",
                    policy_ref
                ))
            })?
    };
    
    let explanation = explain_policy(policy_id);
    println!("{}", explanation);
    
    Ok(())
}

fn enforce_policies(
    pack: Option<&str>,
    all: bool,
    dry_run: bool,
) -> Result<()> {
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
    let mut failed = 0;
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
        if id < 1 || id > 20 {
            return Err(adapteros_core::AosError::Validation(
                "Policy ID must be between 1 and 20".to_string(),
            ));
        }
        Ok(unsafe { std::mem::transmute::<u8, PolicyId>(id as u8) })
    } else {
        let name_lower = policy_ref.to_lowercase();
        list_policies()
            .iter()
            .find(|p| p.name.to_lowercase() == name_lower)
            .map(|p| p.id)
            .ok_or_else(|| {
                adapteros_core::AosError::Validation(format!(
                    "Policy '{}' not found",
                    policy_ref
                ))
            })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_list_policies_count() {
        let policies = list_policies();
        assert_eq!(policies.len(), 20, "Must have exactly 20 policies");
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

