//! Operational runbook generation from Serena memories
//!
//! Provides:
//! - `aosctl ops generate-runbooks` - Generate runbooks from memory files

use crate::output::OutputWriter;
use anyhow::{Context, Result};
use clap::{Args, Subcommand};
use std::collections::HashMap;
use std::fs;
use std::path::PathBuf;

/// Ops command for operational tooling
#[derive(Debug, Clone, Subcommand)]
pub enum OpsCommand {
    /// Generate operational runbooks from Serena memories
    #[command(name = "generate-runbooks", after_help = "\
Examples:
  # Generate runbooks to default location
  aosctl ops generate-runbooks

  # Generate to custom directory
  aosctl ops generate-runbooks --output /tmp/runbooks

  # Dry run - show what would be generated
  aosctl ops generate-runbooks --dry-run
")]
    GenerateRunbooks(GenerateRunbooksArgs),
}

#[derive(Debug, Clone, Args)]
pub struct GenerateRunbooksArgs {
    /// Output directory for generated runbooks
    #[arg(long, default_value = "./docs/runbooks")]
    pub output: PathBuf,

    /// Dry run - show what would be generated without writing
    #[arg(long)]
    pub dry_run: bool,

    /// Path to Serena memories directory
    #[arg(long, default_value = ".serena/memories")]
    pub memories_dir: PathBuf,
}

/// Runbook scenario definition
#[derive(Debug, Clone)]
struct RunbookScenario {
    /// Filename for the runbook
    filename: &'static str,
    /// Title of the runbook
    title: &'static str,
    /// Keywords to search for in memories
    keywords: &'static [&'static str],
    /// Description of the scenario
    description: &'static str,
}

/// Predefined runbook scenarios
const RUNBOOK_SCENARIOS: &[RunbookScenario] = &[
    RunbookScenario {
        filename: "quarantine_triggered.md",
        title: "Quarantine Triggered",
        keywords: &["quarantine", "QuarantineManager", "QuarantineOperation", "policy violation"],
        description: "System has entered quarantine mode due to policy violations",
    },
    RunbookScenario {
        filename: "worker_unhealthy.md",
        title: "Worker Unhealthy",
        keywords: &["worker", "unhealthy", "health", "HealthMonitor", "circuit breaker", "heartbeat"],
        description: "Worker process reporting unhealthy status or failing health checks",
    },
    RunbookScenario {
        filename: "migration_failed.md",
        title: "Database Migration Failed",
        keywords: &["migration", "migrate", "signature", "checksum", "SQLite", "database"],
        description: "Database migration failed during startup or upgrade",
    },
    RunbookScenario {
        filename: "determinism_violation.md",
        title: "Determinism Violation",
        keywords: &["determinism", "seed", "HKDF", "Q15", "reproducible", "replay"],
        description: "Non-deterministic behavior detected during inference or replay",
    },
    RunbookScenario {
        filename: "auth_failure.md",
        title: "Authentication Failure",
        keywords: &["auth", "JWT", "token", "login", "session", "Ed25519", "HMAC"],
        description: "Authentication or authorization failures in API requests",
    },
];

/// Section extracted from a memory file
#[derive(Debug, Clone)]
struct MemorySection {
    source_file: String,
    heading: String,
    content: String,
}

/// Handle ops command
pub async fn handle_ops_command(cmd: OpsCommand, output: &OutputWriter) -> Result<()> {
    match cmd {
        OpsCommand::GenerateRunbooks(args) => generate_runbooks(args, output).await,
    }
}

/// Generate operational runbooks from Serena memories
async fn generate_runbooks(args: GenerateRunbooksArgs, output: &OutputWriter) -> Result<()> {
    output.info("Generating operational runbooks from Serena memories...\n");

    // Verify memories directory exists
    if !args.memories_dir.exists() {
        anyhow::bail!(
            "Memories directory not found: {}",
            args.memories_dir.display()
        );
    }

    // Read all memory files
    let memories = read_all_memories(&args.memories_dir)?;
    output.info(format!("Found {} memory files\n", memories.len()));

    // Extract relevant sections for each scenario
    let mut runbooks_generated = 0;

    for scenario in RUNBOOK_SCENARIOS {
        let sections = extract_relevant_sections(&memories, scenario.keywords);

        if sections.is_empty() {
            output.warning(format!(
                "No relevant content found for: {}\n",
                scenario.title
            ));
            continue;
        }

        let runbook_content = generate_runbook_content(scenario, &sections);

        if args.dry_run {
            output.info(format!(
                "[DRY RUN] Would generate: {} ({} sections)\n",
                scenario.filename,
                sections.len()
            ));
            continue;
        }

        // Create output directory if needed
        if !args.output.exists() {
            fs::create_dir_all(&args.output)
                .with_context(|| format!("Failed to create output directory: {}", args.output.display()))?;
        }

        // Write runbook
        let runbook_path = args.output.join(scenario.filename);
        fs::write(&runbook_path, &runbook_content)
            .with_context(|| format!("Failed to write runbook: {}", runbook_path.display()))?;

        output.success(format!("Generated: {}\n", runbook_path.display()));
        runbooks_generated += 1;
    }

    if args.dry_run {
        output.info(format!(
            "\n[DRY RUN] Would generate {} runbooks\n",
            RUNBOOK_SCENARIOS.len()
        ));
    } else {
        output.success(format!(
            "\nGenerated {} runbooks in {}\n",
            runbooks_generated,
            args.output.display()
        ));
    }

    Ok(())
}

/// Read all markdown files from the memories directory
fn read_all_memories(memories_dir: &PathBuf) -> Result<HashMap<String, String>> {
    let mut memories = HashMap::new();

    for entry in fs::read_dir(memories_dir)? {
        let entry = entry?;
        let path = entry.path();

        if path.extension().map_or(false, |ext| ext == "md") {
            let filename = path.file_name()
                .and_then(|n| n.to_str())
                .unwrap_or("unknown")
                .to_string();

            let content = fs::read_to_string(&path)
                .with_context(|| format!("Failed to read: {}", path.display()))?;

            memories.insert(filename, content);
        }
    }

    Ok(memories)
}

/// Extract sections from memories that match the given keywords
fn extract_relevant_sections(
    memories: &HashMap<String, String>,
    keywords: &[&str],
) -> Vec<MemorySection> {
    let mut sections = Vec::new();

    for (filename, content) in memories {
        // Parse markdown into sections by headings
        let mut current_heading = String::new();
        let mut current_content = String::new();

        for line in content.lines() {
            if line.starts_with('#') {
                // Save previous section if it has relevant content
                if !current_content.is_empty() && section_matches_keywords(&current_content, keywords) {
                    sections.push(MemorySection {
                        source_file: filename.clone(),
                        heading: current_heading.clone(),
                        content: current_content.clone(),
                    });
                }

                // Start new section
                current_heading = line.trim_start_matches('#').trim().to_string();
                current_content = String::new();
            } else {
                current_content.push_str(line);
                current_content.push('\n');
            }
        }

        // Don't forget the last section
        if !current_content.is_empty() && section_matches_keywords(&current_content, keywords) {
            sections.push(MemorySection {
                source_file: filename.clone(),
                heading: current_heading,
                content: current_content,
            });
        }
    }

    sections
}

/// Check if a section content matches any of the keywords
fn section_matches_keywords(content: &str, keywords: &[&str]) -> bool {
    let content_lower = content.to_lowercase();
    keywords.iter().any(|kw| content_lower.contains(&kw.to_lowercase()))
}

/// Generate runbook content from extracted sections
fn generate_runbook_content(scenario: &RunbookScenario, sections: &[MemorySection]) -> String {
    let mut content = String::new();

    // Header
    content.push_str(&format!("# Runbook: {}\n\n", scenario.title));
    content.push_str(&format!("> Auto-generated from Serena memories\n\n"));

    // Overview
    content.push_str("## Overview\n\n");
    content.push_str(&format!("{}\n\n", scenario.description));

    // Detection section
    content.push_str("## Detection\n\n");
    content.push_str("### Symptoms\n\n");
    content.push_str(&format!(
        "- Look for keywords: {}\n",
        scenario.keywords.join(", ")
    ));
    content.push_str("- Check logs for related error codes\n");
    content.push_str("- Monitor health endpoints\n\n");

    content.push_str("### Commands\n\n");
    content.push_str("```bash\n");
    content.push_str("# Check system health\n");
    content.push_str("aosctl doctor\n\n");
    content.push_str("# Check logs\n");
    content.push_str("aosctl log triage var/logs --detailed\n");
    content.push_str("```\n\n");

    // Diagnosis section
    content.push_str("## Diagnosis\n\n");
    content.push_str("### Related Documentation\n\n");

    // Group sections by source file
    let mut by_source: HashMap<&str, Vec<&MemorySection>> = HashMap::new();
    for section in sections {
        by_source
            .entry(&section.source_file)
            .or_default()
            .push(section);
    }

    for (source, file_sections) in &by_source {
        content.push_str(&format!("#### From `{}`\n\n", source));
        for section in file_sections {
            if !section.heading.is_empty() {
                content.push_str(&format!("**{}**\n\n", section.heading));
            }
            // Include a summary of the content (first 500 chars or so)
            let summary = summarize_content(&section.content, 500);
            content.push_str(&format!("{}\n\n", summary));
        }
    }

    // Resolution section
    content.push_str("## Resolution\n\n");
    content.push_str("### General Steps\n\n");

    // Add scenario-specific resolution steps
    match scenario.filename {
        "quarantine_triggered.md" => {
            content.push_str("1. **Identify the violation**: Check logs for the specific policy that triggered quarantine\n");
            content.push_str("2. **Review policy hash**: Verify policy pack integrity with `aosctl policy list`\n");
            content.push_str("3. **Check adapter state**: Run `aosctl adapter list` to see adapter states\n");
            content.push_str("4. **Release quarantine**: Once resolved, use the QuarantineManager API to release\n");
            content.push_str("5. **Verify recovery**: Run `aosctl doctor` to confirm healthy state\n\n");
            content.push_str("### Commands\n\n");
            content.push_str("```bash\n");
            content.push_str("# Check quarantine status\n");
            content.push_str("aosctl quarantine --verbose\n\n");
            content.push_str("# List policy violations\n");
            content.push_str("aosctl policy list\n");
            content.push_str("```\n\n");
        }
        "worker_unhealthy.md" => {
            content.push_str("1. **Check worker status**: Use the status endpoint to get current state\n");
            content.push_str("2. **Review health metrics**: Check memory growth, response times\n");
            content.push_str("3. **Check circuit breaker**: Verify if circuit breaker has tripped\n");
            content.push_str("4. **Drain and restart**: If necessary, drain the worker gracefully\n");
            content.push_str("5. **Verify recovery**: Confirm worker returns to healthy state\n\n");
            content.push_str("### Commands\n\n");
            content.push_str("```bash\n");
            content.push_str("# Check worker status\n");
            content.push_str("aosctl status\n\n");
            content.push_str("# Check system health\n");
            content.push_str("aosctl doctor\n\n");
            content.push_str("# Drain a specific worker (if needed)\n");
            content.push_str("# POST /v1/workers/{id}/drain\n");
            content.push_str("```\n\n");
        }
        "migration_failed.md" => {
            content.push_str("1. **Check migration status**: Review which migration failed\n");
            content.push_str("2. **Verify signatures**: Ensure migrations/signatures.json is valid\n");
            content.push_str("3. **Check database lock**: Clear any stuck migration locks\n");
            content.push_str("4. **Review migration SQL**: Check for syntax or constraint errors\n");
            content.push_str("5. **Retry migration**: Run migration again after fixing issues\n\n");
            content.push_str("### Commands\n\n");
            content.push_str("```bash\n");
            content.push_str("# Check database health\n");
            content.push_str("aosctl db health\n\n");
            content.push_str("# Unlock stuck migrations\n");
            content.push_str("aosctl db unlock\n\n");
            content.push_str("# Retry migrations\n");
            content.push_str("aosctl db migrate\n\n");
            content.push_str("# Verify signatures only\n");
            content.push_str("aosctl db migrate --verify-only\n");
            content.push_str("```\n\n");
        }
        "determinism_violation.md" => {
            content.push_str("1. **Enable debug mode**: Set `AOS_DEBUG_DETERMINISM=1`\n");
            content.push_str("2. **Check seed derivation**: Verify HKDF seed is consistent\n");
            content.push_str("3. **Review Q15 encoding**: Ensure Q15 denominator is 32767.0\n");
            content.push_str("4. **Check sorting**: Verify canonical_score_comparator is used\n");
            content.push_str("5. **Run determinism tests**: Execute the determinism test suite\n\n");
            content.push_str("### Commands\n\n");
            content.push_str("```bash\n");
            content.push_str("# Enable determinism debugging\n");
            content.push_str("export AOS_DEBUG_DETERMINISM=1\n\n");
            content.push_str("# Run determinism check\n");
            content.push_str("aosctl determinism --runs 5\n\n");
            content.push_str("# Audit determinism\n");
            content.push_str("aosctl audit-determinism\n\n");
            content.push_str("# Run determinism test suite\n");
            content.push_str("cargo test --test determinism_core_suite\n");
            content.push_str("```\n\n");
        }
        "auth_failure.md" => {
            content.push_str("1. **Check token validity**: Verify JWT is not expired\n");
            content.push_str("2. **Verify key configuration**: Ensure Ed25519 keys are loaded\n");
            content.push_str("3. **Check session state**: Verify session exists in database\n");
            content.push_str("4. **Review revocation list**: Check if token is revoked\n");
            content.push_str("5. **Re-authenticate**: Issue new token if necessary\n\n");
            content.push_str("### Commands\n\n");
            content.push_str("```bash\n");
            content.push_str("# Check auth status\n");
            content.push_str("aosctl auth status\n\n");
            content.push_str("# Login again\n");
            content.push_str("aosctl auth login\n\n");
            content.push_str("# Check system health (includes auth)\n");
            content.push_str("aosctl doctor\n");
            content.push_str("```\n\n");
        }
        _ => {
            content.push_str("1. Review the diagnosis section above\n");
            content.push_str("2. Check system logs with `aosctl log triage`\n");
            content.push_str("3. Run `aosctl doctor` for health check\n");
            content.push_str("4. Consult the relevant documentation\n\n");
        }
    }

    // Related error codes
    content.push_str("## Related Error Codes\n\n");
    content.push_str("Use `aosctl explain <code>` for detailed information:\n\n");

    match scenario.filename {
        "quarantine_triggered.md" => {
            content.push_str("- `POLICY_VIOLATION` - Policy check failed\n");
            content.push_str("- `DETERMINISM_VIOLATION` - Non-deterministic behavior\n");
            content.push_str("- `ISOLATION_VIOLATION` - Tenant isolation breach\n");
        }
        "worker_unhealthy.md" => {
            content.push_str("- `WORKER_NOT_AVAILABLE` - Worker not responding\n");
            content.push_str("- `CIRCUIT_BREAKER_OPEN` - Too many failures\n");
            content.push_str("- `BACKPRESSURE` - System under load\n");
        }
        "migration_failed.md" => {
            content.push_str("- `DATABASE_ERROR` - Database operation failed\n");
            content.push_str("- `VALIDATION_ERROR` - Migration validation failed\n");
        }
        "determinism_violation.md" => {
            content.push_str("- `DETERMINISM_VIOLATION` - Non-deterministic output\n");
            content.push_str("- `E2xxx` - Policy/Determinism error codes\n");
        }
        "auth_failure.md" => {
            content.push_str("- `TOKEN_MISSING` - No auth token provided\n");
            content.push_str("- `TOKEN_INVALID` - Token validation failed\n");
            content.push_str("- `TOKEN_EXPIRED` - Token has expired\n");
            content.push_str("- `SESSION_EXPIRED` - Session no longer valid\n");
        }
        _ => {}
    }
    content.push('\n');

    // Footer
    content.push_str("---\n\n");
    content.push_str("*Generated from Serena memories. For the most up-to-date information, ");
    content.push_str("consult the source documentation in `.serena/memories/`.*\n");

    content
}

/// Summarize content to a maximum length, preserving word and char boundaries
fn summarize_content(content: &str, max_len: usize) -> String {
    let trimmed = content.trim();
    if trimmed.len() <= max_len {
        return trimmed.to_string();
    }

    // Find a valid char boundary at or before max_len
    let mut end = max_len;
    while end > 0 && !trimmed.is_char_boundary(end) {
        end -= 1;
    }

    if end == 0 {
        return "...".to_string();
    }

    // Find a good breaking point (space) within the valid range
    let truncated = &trimmed[..end];
    if let Some(last_space) = truncated.rfind(' ') {
        format!("{}...", &truncated[..last_space])
    } else {
        format!("{}...", truncated)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_section_matches_keywords() {
        let content = "The QuarantineManager handles policy violations";
        assert!(section_matches_keywords(content, &["quarantine"]));
        assert!(section_matches_keywords(content, &["policy"]));
        assert!(!section_matches_keywords(content, &["database"]));
    }

    #[test]
    fn test_summarize_content() {
        let short = "Short content";
        assert_eq!(summarize_content(short, 100), "Short content");

        let long = "This is a much longer piece of content that should be truncated at a word boundary";
        let summary = summarize_content(long, 30);
        assert!(summary.ends_with("..."));
        assert!(summary.len() <= 33); // 30 + "..."

        // Test with unicode characters (multi-byte)
        let unicode = "Test with unicode: \u{2500}\u{2500}\u{2500} symbols";
        let summary_unicode = summarize_content(unicode, 25);
        assert!(summary_unicode.ends_with("..."));
    }

    #[test]
    fn test_runbook_scenarios_defined() {
        assert!(!RUNBOOK_SCENARIOS.is_empty());
        for scenario in RUNBOOK_SCENARIOS {
            assert!(!scenario.filename.is_empty());
            assert!(!scenario.title.is_empty());
            assert!(!scenario.keywords.is_empty());
            assert!(!scenario.description.is_empty());
        }
    }
}
