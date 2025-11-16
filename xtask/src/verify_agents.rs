//! Agent verification system
//!
//! Validates that all six agents (A-F) delivered their agreed work
//! with machine-readable PASS/FAIL reports.

use anyhow::Result;
use clap::Parser;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::PathBuf;

pub mod agent_a;
pub mod agent_b;
pub mod agent_c;
pub mod agent_d;
pub mod agent_e;
pub mod agent_f;
pub mod baseline;
pub mod report;
pub mod verify_deterministic_exec;

#[derive(Parser, Debug)]
#[command(name = "verify-agents")]
#[command(about = "Verify that Agents A-F delivered all agreed work")]
pub struct VerifyAgentsArgs {
    /// Output directory for verification artifacts
    #[arg(long, default_value = "target/verify")]
    pub artifacts: PathBuf,

    /// Fail on performance regression
    #[arg(long)]
    pub fail_on_regression: bool,

    /// Timeout for entire verification in seconds
    #[arg(long, default_value = "300")]
    pub timeout: u64,

    /// Metrics bearer token for testing
    #[arg(long, default_value = "testtoken")]
    pub metrics_token: String,

    /// Skip runtime checks (server start/stop)
    #[arg(long)]
    pub static_only: bool,

    /// Assume server is already running at this URL
    #[arg(long)]
    pub assume_running: Option<String>,

    /// Allow updating performance baselines
    #[arg(long)]
    pub update_baselines: bool,

    /// Skip GPU counter checks
    #[arg(long)]
    pub no_gpu: bool,

    /// Tenant ID for scoped queries
    #[arg(long)]
    pub tenant: Option<String>,

    /// PID file path for server lock
    #[arg(long, default_value = "var/aos-cp.pid")]
    pub pid_file: PathBuf,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "UPPERCASE")]
pub enum CheckStatus {
    Pass,
    Fail,
    Skip,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Check {
    pub name: String,
    pub status: CheckStatus,
    pub evidence: Vec<String>,
    pub notes: String,
}

impl Check {
    pub fn pass(name: impl Into<String>, evidence: Vec<String>) -> Self {
        Self {
            name: name.into(),
            status: CheckStatus::Pass,
            evidence,
            notes: String::new(),
        }
    }

    pub fn fail(name: impl Into<String>, evidence: Vec<String>, notes: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            status: CheckStatus::Fail,
            evidence,
            notes: notes.into(),
        }
    }

    pub fn skip(name: impl Into<String>, notes: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            status: CheckStatus::Skip,
            evidence: vec![],
            notes: notes.into(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Section {
    pub name: String,
    pub checks: Vec<Check>,
}

impl Section {
    pub fn new(name: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            checks: Vec::new(),
        }
    }

    pub fn add_check(&mut self, check: Check) {
        self.checks.push(check);
    }
}

#[derive(Debug, Serialize, Deserialize)]
pub struct VerificationSummary {
    pub pass: usize,
    pub fail: usize,
    pub skip: usize,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct VerificationReport {
    pub timestamp: String,
    pub summary: VerificationSummary,
    pub sections: Vec<Section>,
    pub artifacts: HashMap<String, String>,
}

impl VerificationReport {
    pub fn new() -> Self {
        Self {
            timestamp: chrono::Utc::now().to_rfc3339(),
            summary: VerificationSummary {
                pass: 0,
                fail: 0,
                skip: 0,
            },
            sections: Vec::new(),
            artifacts: HashMap::new(),
        }
    }

    pub fn add_section(&mut self, section: Section) {
        // Update summary
        for check in &section.checks {
            match check.status {
                CheckStatus::Pass => self.summary.pass += 1,
                CheckStatus::Fail => self.summary.fail += 1,
                CheckStatus::Skip => self.summary.skip += 1,
            }
        }
        self.sections.push(section);
    }

    pub fn _add_artifact(&mut self, name: impl Into<String>, path: impl Into<String>) {
        self.artifacts.insert(name.into(), path.into());
    }

    pub fn exit_code(&self) -> i32 {
        if self.summary.fail > 0 {
            1 // Functional failure
        } else {
            0 // All passed or skipped
        }
    }
}

/// Run all agent verification checks
pub async fn run(args: VerifyAgentsArgs) -> Result<VerificationReport> {
    let mut report = VerificationReport::new();

    // Create artifacts directory
    std::fs::create_dir_all(&args.artifacts)?;

    println!("=== AdapterOS Agent Verification ===\n");

    // Run baseline checks first
    println!("Running baseline checks...");
    let baseline_section = baseline::run(&args).await?;
    report.add_section(baseline_section);

    // Agent A: Kernel & Determinism
    println!("\nRunning Agent A checks (Kernel & Determinism)...");
    let agent_a_section = agent_a::run(&args).await?;
    report.add_section(agent_a_section);

    // Agent B: Backend & Control Plane
    println!("\nRunning Agent B checks (Backend & Control Plane)...");
    let agent_b_section = agent_b::run(&args).await?;
    report.add_section(agent_b_section);

    // Agent C: Adapters & Routing
    println!("\nRunning Agent C checks (Adapters & Routing)...");
    let agent_c_section = agent_c::run(&args).await?;
    report.add_section(agent_c_section);

    // Deterministic Execution & Multi-Agent Coordination
    println!("\nRunning Deterministic Execution verification...");
    let deterministic_exec_section = verify_deterministic_exec::run(&args).await?;
    report.add_section(deterministic_exec_section);

    // Agent D: UI/UX/Observability
    println!("\nRunning Agent D checks (UI/UX/Observability)...");
    let agent_d_section = agent_d::run(&args).await?;
    report.add_section(agent_d_section);

    // Agent E: Testing/Deployment/Compliance
    println!("\nRunning Agent E checks (Testing/Deployment/Compliance)...");
    let agent_e_section = agent_e::run(&args).await?;
    report.add_section(agent_e_section);

    // Agent F: Adapter Lifecycle & TTL
    println!("\nRunning Agent F checks (Adapter Lifecycle & TTL)...");
    let agent_f_section = agent_f::run(&args).await?;
    report.add_section(agent_f_section);

    // Generate reports
    println!("\nGenerating reports...");
    report::generate(&report, &args.artifacts)?;

    Ok(report)
}
