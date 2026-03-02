//! Review management commands for human-in-the-loop workflows
//!
//! These commands enable operators to list, inspect, and respond to
//! items that require human review (paused inferences, dataset approvals, etc.)

use super::review_tui;
use crate::output::OutputWriter;
use adapteros_api_types::review::{
    InferenceStateResponse, IssueSeverity, ListPausedResponse, PausedInferenceInfo, Review,
    ReviewAssessment, ReviewContextExport, ReviewIssue, ReviewScope, SubmitReviewRequest,
    SubmitReviewResponse,
};
use adapteros_core::Result;
use clap::{Subcommand, ValueEnum};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use tracing::info;

/// Review command variants
#[derive(Debug, Subcommand, Clone)]
pub enum ReviewCommand {
    /// List items pending review
    #[command(
        after_help = "Examples:\n  aosctl review list\n  aosctl review list --kind review-needed\n  aosctl review list --json"
    )]
    List {
        /// Filter by pause kind
        #[arg(long, value_enum)]
        kind: Option<PauseKindFilter>,

        /// Output JSON format
        #[arg(long)]
        json: bool,

        /// API base URL
        #[arg(long, env = "AOS_API_URL", default_value = "http://127.0.0.1:18080")]
        base_url: String,
    },

    /// Get details for a specific paused item
    #[command(
        after_help = "Examples:\n  aosctl review get pause-abc123\n  aosctl review get pause-abc123 --json"
    )]
    Get {
        /// Pause ID to inspect
        #[arg()]
        pause_id: String,

        /// Output JSON format
        #[arg(long)]
        json: bool,

        /// API base URL
        #[arg(long, env = "AOS_API_URL", default_value = "http://127.0.0.1:18080")]
        base_url: String,
    },

    /// Submit a review response
    #[command(
        after_help = "Examples:\n  aosctl review submit pause-abc123 --approve\n  aosctl review submit pause-abc123 --reject --comment 'Security issue'\n  aosctl review submit pause-abc123 --needs-changes --issue 'Missing error handling'"
    )]
    Submit {
        /// Pause ID to respond to
        #[arg()]
        pause_id: String,

        /// Approve the item
        #[arg(long, conflicts_with_all = ["reject", "needs_changes"])]
        approve: bool,

        /// Reject the item
        #[arg(long, conflicts_with_all = ["approve", "needs_changes"])]
        reject: bool,

        /// Request changes
        #[arg(long, conflicts_with_all = ["approve", "reject"])]
        needs_changes: bool,

        /// Review comment
        #[arg(long, short = 'c')]
        comment: Option<String>,

        /// Issues found (can be repeated)
        #[arg(long, short = 'i')]
        issue: Vec<String>,

        /// Suggestions (can be repeated)
        #[arg(long, short = 's')]
        suggestion: Vec<String>,

        /// Reviewer name
        #[arg(long, default_value = "cli-user")]
        reviewer: String,

        /// API base URL
        #[arg(long, env = "AOS_API_URL", default_value = "http://127.0.0.1:18080")]
        base_url: String,
    },

    /// Export review context for external review (e.g., Claude Code)
    #[command(
        after_help = "Examples:\n  aosctl review export pause-abc123\n  aosctl review export pause-abc123 -o context.json"
    )]
    Export {
        /// Pause ID to export
        #[arg()]
        pause_id: String,

        /// Output file (stdout if not specified)
        #[arg(long, short = 'o')]
        output: Option<PathBuf>,

        /// API base URL
        #[arg(long, env = "AOS_API_URL", default_value = "http://127.0.0.1:18080")]
        base_url: String,
    },

    /// Import a review response from file (e.g., from Claude Code)
    #[command(after_help = "Examples:\n  aosctl review import pause-abc123 -f response.json")]
    Import {
        /// Pause ID to respond to
        #[arg()]
        pause_id: String,

        /// Input file with review response
        #[arg(long, short = 'f')]
        file: PathBuf,

        /// Reviewer name
        #[arg(long, default_value = "external")]
        reviewer: String,

        /// API base URL
        #[arg(long, env = "AOS_API_URL", default_value = "http://127.0.0.1:18080")]
        base_url: String,
    },

    /// Launch TUI for active learning sample review
    #[command(after_help = "Examples:\n  aosctl review tui")]
    Tui {
        /// Active learning directory (optional)
        #[arg(long, env = "AOS_ACTIVE_LEARNING_DIR")]
        dir: Option<PathBuf>,
    },
}

/// Filter for pause kinds
#[derive(Debug, Clone, Copy, ValueEnum)]
pub enum PauseKindFilter {
    /// Items needing human review
    ReviewNeeded,
    /// Items awaiting policy approval
    PolicyApproval,
    /// Items waiting on resources
    ResourceWait,
    /// User-initiated pauses
    UserRequested,
    /// High-severity threat detected, requires human review before continuing
    ThreatEscalation,
}

impl std::fmt::Display for PauseKindFilter {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::ReviewNeeded => write!(f, "ReviewNeeded"),
            Self::PolicyApproval => write!(f, "PolicyApproval"),
            Self::ResourceWait => write!(f, "ResourceWait"),
            Self::UserRequested => write!(f, "UserRequested"),
            Self::ThreatEscalation => write!(f, "ThreatEscalation"),
        }
    }
}

/// Imported review response from external reviewer
#[derive(Debug, Serialize, Deserialize)]
pub struct ImportedReviewResponse {
    pub assessment: String,
    pub issues: Option<Vec<ImportedIssue>>,
    pub suggestions: Option<Vec<String>>,
    pub comments: Option<String>,
    pub confidence: Option<f32>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct ImportedIssue {
    pub severity: Option<String>,
    pub category: Option<String>,
    pub description: String,
    pub location: Option<String>,
    pub suggested_fix: Option<String>,
}

/// Handle review commands
pub async fn handle_review_command(cmd: ReviewCommand, output: &OutputWriter) -> Result<()> {
    let command_name = get_review_command_name(&cmd);

    info!(command = ?cmd, "Handling review command");

    // Emit telemetry
    if let Err(e) = crate::cli_telemetry::emit_cli_command(&command_name, None, true).await {
        tracing::debug!(error = %e, command = %command_name, "Telemetry emit failed (non-fatal)");
    }

    match cmd {
        ReviewCommand::List {
            kind,
            json,
            base_url,
        } => list_reviews(kind, json, &base_url, output).await,
        ReviewCommand::Get {
            pause_id,
            json,
            base_url,
        } => get_review(&pause_id, json, &base_url, output).await,
        ReviewCommand::Submit {
            pause_id,
            approve,
            reject,
            needs_changes,
            comment,
            issue,
            suggestion,
            reviewer,
            base_url,
        } => {
            submit_review(
                &pause_id,
                approve,
                reject,
                needs_changes,
                comment,
                issue,
                suggestion,
                &reviewer,
                &base_url,
                output,
            )
            .await
        }
        ReviewCommand::Export {
            pause_id,
            output: output_path,
            base_url,
        } => export_review(&pause_id, output_path, &base_url, output).await,
        ReviewCommand::Import {
            pause_id,
            file,
            reviewer,
            base_url,
        } => import_review(&pause_id, &file, &reviewer, &base_url, output).await,
        ReviewCommand::Tui { dir } => review_tui::run_review_tui(dir, output).await,
    }
}

async fn list_reviews(
    kind: Option<PauseKindFilter>,
    json_output: bool,
    base_url: &str,
    output: &OutputWriter,
) -> Result<()> {
    let client = reqwest::Client::new();
    let mut url = format!("{}/v1/reviews/paused", base_url);

    if let Some(k) = kind {
        url = format!("{}?kind={}", url, k);
    }

    let resp = client.get(&url).send().await;

    match resp {
        Ok(response) if response.status().is_success() => {
            let paused: ListPausedResponse = response.json().await.map_err(|e| {
                adapteros_core::AosError::internal(format!("JSON parse error: {}", e))
            })?;

            if json_output {
                output.print_json(&paused)?;
            } else if paused.paused.is_empty() {
                output.success("No items pending review");
            } else {
                output.section(format!("Pending Reviews ({})", paused.total));
                println!();

                for item in &paused.paused {
                    print_paused_item(item, output);
                    println!();
                }
            }
        }
        Ok(response) => {
            let status = response.status();
            let body = response.text().await.unwrap_or_default();
            output.error(format!("API error ({}): {}", status, body));
        }
        Err(e) => {
            // API not available - show placeholder for development
            output.warning(format!("API not available: {}", e));
            output.info("Review API unavailable; ensure the server is running and review endpoints are enabled");
            println!();
            output.info("Expected endpoint: GET /v1/reviews/paused");
        }
    }

    Ok(())
}

async fn get_review(
    pause_id: &str,
    json_output: bool,
    base_url: &str,
    output: &OutputWriter,
) -> Result<()> {
    let client = reqwest::Client::new();
    let url = format!("{}/v1/reviews/{}", base_url, pause_id);

    let resp = client.get(&url).send().await;

    match resp {
        Ok(response) if response.status().is_success() => {
            let state: InferenceStateResponse = response.json().await.map_err(|e| {
                adapteros_core::AosError::internal(format!("JSON parse error: {}", e))
            })?;

            if json_output {
                output.print_json(&state)?;
            } else {
                output.section("Review Details");
                output.kv("Inference ID", &state.inference_id);
                output.kv("State", &format!("{:?}", state.state));
                if let Some(at) = &state.paused_at {
                    output.kv("Paused At", at);
                }
                if let Some(dur) = state.paused_duration_secs {
                    output.kv("Duration", &format!("{}s", dur));
                }
            }
        }
        Ok(response) if response.status().as_u16() == 404 => {
            output.error(format!("Pause ID not found: {}", pause_id));
        }
        Ok(response) => {
            let status = response.status();
            let body = response.text().await.unwrap_or_default();
            output.error(format!("API error ({}): {}", status, body));
        }
        Err(e) => {
            output.warning(format!("API not available: {}", e));
            output.info("Expected endpoint: GET /v1/reviews/{pause_id}");
        }
    }

    Ok(())
}

#[allow(clippy::too_many_arguments)]
async fn submit_review(
    pause_id: &str,
    approve: bool,
    reject: bool,
    needs_changes: bool,
    comment: Option<String>,
    issues: Vec<String>,
    suggestions: Vec<String>,
    reviewer: &str,
    base_url: &str,
    output: &OutputWriter,
) -> Result<()> {
    // Determine assessment
    let assessment = if approve {
        if suggestions.is_empty() {
            ReviewAssessment::Approved
        } else {
            ReviewAssessment::ApprovedWithSuggestions
        }
    } else if reject {
        ReviewAssessment::Rejected
    } else if needs_changes {
        ReviewAssessment::NeedsChanges
    } else {
        output.error("Must specify --approve, --reject, or --needs-changes");
        return Ok(());
    };

    // Build issues
    let review_issues: Vec<ReviewIssue> = issues
        .into_iter()
        .map(|desc| ReviewIssue {
            severity: IssueSeverity::Medium,
            category: ReviewScope::Logic,
            description: desc,
            location: None,
            suggested_fix: None,
        })
        .collect();

    let review = Review {
        assessment,
        issues: review_issues,
        suggestions,
        comments: comment,
        confidence: None,
    };

    let request = SubmitReviewRequest {
        pause_id: pause_id.to_string(),
        review,
        reviewer: reviewer.to_string(),
    };

    let client = reqwest::Client::new();
    let url = format!("{}/v1/reviews/submit", base_url);

    let resp = client.post(&url).json(&request).send().await;

    match resp {
        Ok(response) if response.status().is_success() => {
            let result: SubmitReviewResponse = response.json().await.map_err(|e| {
                adapteros_core::AosError::internal(format!("JSON parse error: {}", e))
            })?;

            if result.accepted {
                output.success(format!("Review submitted for {}", pause_id));
                output.kv("New State", &format!("{:?}", result.new_state));
            } else {
                output.warning("Review not accepted");
                if let Some(msg) = result.message {
                    output.info(&msg);
                }
            }
        }
        Ok(response) => {
            let status = response.status();
            let body = response.text().await.unwrap_or_default();
            output.error(format!("API error ({}): {}", status, body));
        }
        Err(e) => {
            output.warning(format!("API not available: {}", e));
            output.info("Expected endpoint: POST /v1/reviews/submit");
            println!();
            output.info("Review that would be submitted:");
            output.print_json(&request)?;
        }
    }

    Ok(())
}

async fn export_review(
    pause_id: &str,
    output_path: Option<PathBuf>,
    base_url: &str,
    output: &OutputWriter,
) -> Result<()> {
    let client = reqwest::Client::new();
    let url = format!("{}/v1/reviews/{}/context", base_url, pause_id);

    let resp = client.get(&url).send().await;

    let exported = match resp {
        Ok(response) if response.status().is_success() => response
            .json::<ReviewContextExport>()
            .await
            .map_err(|e| adapteros_core::AosError::internal(format!("JSON parse error: {}", e)))?,
        Ok(response) if response.status().as_u16() == 404 => {
            output.error(format!("Pause ID not found: {}", pause_id));
            return Ok(());
        }
        Ok(response) => {
            let status = response.status();
            let body = response.text().await.unwrap_or_default();
            output.error(format!("API error ({}): {}", status, body));
            return Ok(());
        }
        Err(_) => {
            // Generate placeholder for development
            ReviewContextExport {
                pause_id: pause_id.to_string(),
                inference_id: "inf-placeholder".to_string(),
                kind: "ReviewNeeded".to_string(),
                paused_at: chrono::Utc::now().to_rfc3339(),
                duration_secs: 0,
                code: Some("// Code to review would appear here".to_string()),
                question: Some("Is this implementation correct?".to_string()),
                scope: vec!["Logic".to_string(), "Security".to_string()],
                metadata: None,
                instructions: format!(
                    "Review this item and respond with a JSON file containing:\n\
                     - assessment: Approved | ApprovedWithSuggestions | NeedsChanges | Rejected\n\
                     - issues: [{{severity, description, suggested_fix}}]\n\
                     - suggestions: [string]\n\
                     - comments: string\n\n\
                     Then import with: aosctl review import {} -f response.json",
                    pause_id
                ),
            }
        }
    };

    let json = serde_json::to_string_pretty(&exported)?;

    if let Some(path) = output_path {
        std::fs::write(&path, &json)?;
        output.success(format!("Exported review context to {}", path.display()));
    } else {
        println!("{}", json);
    }

    Ok(())
}

async fn import_review(
    pause_id: &str,
    file: &PathBuf,
    reviewer: &str,
    base_url: &str,
    output: &OutputWriter,
) -> Result<()> {
    // Read and parse the response file
    let content = std::fs::read_to_string(file)?;
    let imported: ImportedReviewResponse = serde_json::from_str(&content)?;

    // Convert to API types
    let assessment = match imported.assessment.to_lowercase().as_str() {
        "approved" => ReviewAssessment::Approved,
        "approvedwithsuggestions" | "approved_with_suggestions" => {
            ReviewAssessment::ApprovedWithSuggestions
        }
        "needschanges" | "needs_changes" => ReviewAssessment::NeedsChanges,
        "rejected" => ReviewAssessment::Rejected,
        "inconclusive" => ReviewAssessment::Inconclusive,
        other => {
            output.error(format!("Unknown assessment: {}", other));
            return Ok(());
        }
    };

    let issues: Vec<ReviewIssue> = imported
        .issues
        .unwrap_or_default()
        .into_iter()
        .map(|i| ReviewIssue {
            severity: parse_severity(&i.severity.unwrap_or_default()),
            category: parse_scope(&i.category.unwrap_or_default()),
            description: i.description,
            location: i.location,
            suggested_fix: i.suggested_fix,
        })
        .collect();

    let review = Review {
        assessment,
        issues,
        suggestions: imported.suggestions.unwrap_or_default(),
        comments: imported.comments,
        confidence: imported.confidence,
    };

    let request = SubmitReviewRequest {
        pause_id: pause_id.to_string(),
        review,
        reviewer: reviewer.to_string(),
    };

    // Submit
    let client = reqwest::Client::new();
    let url = format!("{}/v1/reviews/submit", base_url);

    let resp = client.post(&url).json(&request).send().await;

    match resp {
        Ok(response) if response.status().is_success() => {
            let result: SubmitReviewResponse = response.json().await.map_err(|e| {
                adapteros_core::AosError::internal(format!("JSON parse error: {}", e))
            })?;

            if result.accepted {
                output.success(format!("Imported review for {}", pause_id));
                output.kv("New State", &format!("{:?}", result.new_state));
            } else {
                output.warning("Review not accepted");
                if let Some(msg) = result.message {
                    output.info(&msg);
                }
            }
        }
        Ok(response) => {
            let status = response.status();
            let body = response.text().await.unwrap_or_default();
            output.error(format!("API error ({}): {}", status, body));
        }
        Err(e) => {
            output.warning(format!("API not available: {}", e));
            output.info("Review parsed successfully. Would submit:");
            output.print_json(&request)?;
        }
    }

    Ok(())
}

fn print_paused_item(item: &PausedInferenceInfo, output: &OutputWriter) {
    output.kv("Pause ID", &item.pause_id);
    output.kv("Inference ID", &item.inference_id);
    output.kv("Kind", &format!("{:?}", item.kind));
    output.kv("Paused At", &item.paused_at);
    output.kv("Duration", &format!("{}s", item.duration_secs));
    if let Some(preview) = &item.context_preview {
        output.kv("Preview", preview);
    }
}

fn parse_severity(s: &str) -> IssueSeverity {
    match s.to_lowercase().as_str() {
        "info" => IssueSeverity::Info,
        "low" => IssueSeverity::Low,
        "medium" => IssueSeverity::Medium,
        "high" => IssueSeverity::High,
        "critical" => IssueSeverity::Critical,
        _ => IssueSeverity::Medium,
    }
}

fn parse_scope(s: &str) -> ReviewScope {
    match s.to_lowercase().as_str() {
        "logic" => ReviewScope::Logic,
        "edgecases" | "edge_cases" => ReviewScope::EdgeCases,
        "security" => ReviewScope::Security,
        "performance" => ReviewScope::Performance,
        "style" => ReviewScope::Style,
        "apidesign" | "api_design" => ReviewScope::ApiDesign,
        "testing" => ReviewScope::Testing,
        "documentation" => ReviewScope::Documentation,
        _ => ReviewScope::Logic,
    }
}

fn get_review_command_name(cmd: &ReviewCommand) -> String {
    match cmd {
        ReviewCommand::List { .. } => "review-list",
        ReviewCommand::Get { .. } => "review-get",
        ReviewCommand::Submit { .. } => "review-submit",
        ReviewCommand::Export { .. } => "review-export",
        ReviewCommand::Import { .. } => "review-import",
        ReviewCommand::Tui { .. } => "review-tui",
    }
    .to_string()
}
