//! Report generation for orchestrator results

use adapteros_core::time;
use anyhow::Result;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fs;
use std::path::Path;

/// Report format
#[derive(Debug, Clone, Copy)]
pub enum ReportFormat {
    Json,
    Markdown,
}

/// Gate result
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GateResult {
    pub passed: bool,
    pub message: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub evidence: Option<String>,
}

impl GateResult {
    pub fn passed() -> Self {
        Self {
            passed: true,
            message: "Gate passed".to_string(),
            evidence: None,
        }
    }

    pub fn failed(message: String) -> Self {
        Self {
            passed: false,
            message,
            evidence: None,
        }
    }
}

/// Full gate report
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GateReport {
    pub cpid: String,
    pub timestamp: String,
    pub gates: HashMap<String, GateResult>,
    pub all_passed: bool,
}

impl GateReport {
    pub fn new(cpid: String) -> Self {
        Self {
            cpid,
            timestamp: time::now_rfc3339(),
            gates: HashMap::new(),
            all_passed: true,
        }
    }

    pub fn add_result(&mut self, gate_name: String, result: GateResult) {
        if !result.passed {
            self.all_passed = false;
        }
        self.gates.insert(gate_name, result);
    }

    /// Render as JSON
    pub fn to_json(&self) -> Result<String> {
        Ok(serde_json::to_string_pretty(self)?)
    }

    /// Render as Markdown
    pub fn to_markdown(&self) -> String {
        let mut md = String::new();

        md.push_str("# AdapterOS Promotion Gate Report\n\n");
        md.push_str(&format!("**CPID:** `{}`  \n", self.cpid));
        md.push_str(&format!("**Timestamp:** {}  \n", self.timestamp));
        md.push_str(&format!(
            "**Status:** {}  \n\n",
            if self.all_passed {
                "✅ PASSED"
            } else {
                "❌ FAILED"
            }
        ));

        md.push_str("## Gate Results\n\n");
        md.push_str("| Gate | Status | Message |\n");
        md.push_str("|------|--------|----------|\n");

        let mut gates: Vec<_> = self.gates.iter().collect();
        gates.sort_by_key(|(name, _)| *name);

        for (name, result) in gates {
            let status = if result.passed {
                "✅ PASS"
            } else {
                "❌ FAIL"
            };
            md.push_str(&format!("| {} | {} | {} |\n", name, status, result.message));
        }

        md.push('\n');

        if !self.all_passed {
            md.push_str("## Action Required\n\n");
            md.push_str("One or more gates failed. Address the issues above before promotion.\n");
        } else {
            md.push_str("## Summary\n\n");
            md.push_str("All gates passed. CPID is ready for promotion.\n");
        }

        md
    }

    /// Write report to file
    pub fn write_to_file(&self, path: &Path, format: ReportFormat) -> Result<()> {
        let content = match format {
            ReportFormat::Json => self.to_json()?,
            ReportFormat::Markdown => self.to_markdown(),
        };

        fs::write(path, content)?;
        Ok(())
    }
}

