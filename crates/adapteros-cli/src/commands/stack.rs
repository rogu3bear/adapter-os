//! Adapter stack management commands

use crate::output::OutputWriter;
use adapteros_core::{AosError, Result};
use clap::Subcommand;
use comfy_table::{modifiers::UTF8_ROUND_CORNERS, presets::UTF8_FULL, Table};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use tracing::{error, info};

/// Request to create a new adapter stack
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CreateStackRequest {
    pub name: String,
    pub description: Option<String>,
    pub adapter_ids: Vec<String>,
    pub workflow_type: Option<WorkflowType>,
    pub metadata: Option<HashMap<String, String>>,
}

/// Response for adapter stack operations
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StackResponse {
    pub schema_version: String,
    pub id: String,
    pub tenant_id: String,
    pub name: String,
    pub description: Option<String>,
    pub adapter_ids: Vec<String>,
    pub workflow_type: Option<WorkflowType>,
    pub created_at: String,
    pub updated_at: String,
    pub is_active: bool,
    pub version: i64,
    pub lifecycle_state: String,
    #[serde(default)]
    pub warnings: Vec<String>,
}

/// Workflow type for adapter stacks
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum WorkflowType {
    Parallel,
    UpstreamDownstream,
    Sequential,
}

#[derive(Debug, Subcommand, Clone)]
pub enum StackCommand {
    /// List all adapter stacks
    #[command(
        after_help = "Examples:\n  aosctl stack list\n  aosctl stack list --tenant dev\n  aosctl stack list --json"
    )]
    List {
        /// Tenant ID (defaults to all tenants)
        #[arg(long)]
        tenant: Option<String>,

        /// Output format
        #[arg(long)]
        json: bool,
    },

    /// Create a new adapter stack
    #[command(
        after_help = "Examples:\n  aosctl stack create --name my-stack --adapters adapter1,adapter2,adapter3\n  aosctl stack create --name my-stack --adapters adapter1,adapter2 --description \"Production stack\"\n  aosctl stack create --name my-stack --adapters adapter1 --workflow parallel"
    )]
    Create {
        /// Stack name
        #[arg(long)]
        name: String,

        /// Adapter IDs (comma-separated)
        #[arg(long, value_delimiter = ',')]
        adapters: Vec<String>,

        /// Description
        #[arg(long)]
        description: Option<String>,

        /// Workflow type (parallel, upstream_downstream, sequential)
        #[arg(long)]
        workflow: Option<String>,

        /// Base URL for API
        #[arg(long, default_value = "http://127.0.0.1:8080/api")]
        base_url: String,
    },

    /// Show stack details
    #[command(
        after_help = "Examples:\n  aosctl stack show <stack-id>\n  aosctl stack show <stack-id> --json"
    )]
    Show {
        /// Stack ID
        stack_id: String,

        /// Output format
        #[arg(long)]
        json: bool,

        /// Base URL for API
        #[arg(long, default_value = "http://127.0.0.1:8080/api")]
        base_url: String,
    },

    /// Delete an adapter stack
    #[command(
        after_help = "Examples:\n  aosctl stack delete <stack-id>\n  aosctl stack delete <stack-id> --confirm"
    )]
    Delete {
        /// Stack ID
        stack_id: String,

        /// Skip confirmation prompt
        #[arg(long)]
        confirm: bool,

        /// Base URL for API
        #[arg(long, default_value = "http://127.0.0.1:8080/api")]
        base_url: String,
    },

    /// Activate an adapter stack
    #[command(
        after_help = "Examples:\n  aosctl stack activate <stack-id>\n  aosctl stack activate <stack-id> --tenant dev"
    )]
    Activate {
        /// Stack ID
        stack_id: String,

        /// Tenant ID (defaults to current user's tenant)
        #[arg(long)]
        tenant: Option<String>,

        /// Base URL for API
        #[arg(long, default_value = "http://127.0.0.1:8080/api")]
        base_url: String,
    },

    /// Deactivate current adapter stack
    #[command(
        after_help = "Examples:\n  aosctl stack deactivate\n  aosctl stack deactivate --tenant dev"
    )]
    Deactivate {
        /// Tenant ID (defaults to current user's tenant)
        #[arg(long)]
        tenant: Option<String>,

        /// Base URL for API
        #[arg(long, default_value = "http://127.0.0.1:8080/api")]
        base_url: String,
    },

    /// Set stack as default for tenant
    #[command(
        after_help = "Examples:\n  aosctl stack set-default <stack-id>\n  aosctl stack set-default <stack-id> --tenant dev"
    )]
    SetDefault {
        /// Stack ID
        stack_id: String,

        /// Tenant ID (defaults to current user's tenant)
        #[arg(long)]
        tenant: Option<String>,

        /// Base URL for API
        #[arg(long, default_value = "http://127.0.0.1:8080/api")]
        base_url: String,
    },
}

/// Handle stack commands
pub async fn handle_stack_command(cmd: StackCommand, output: &OutputWriter) -> Result<()> {
    let command_name = get_stack_command_name(&cmd);
    let tenant_id = extract_tenant_from_stack_command(&cmd);

    info!(command = ?cmd, "Handling stack command");

    // Emit telemetry
    let _ = crate::cli_telemetry::emit_cli_command(&command_name, tenant_id.as_deref(), true).await;

    match cmd {
        StackCommand::List { tenant, json } => list_stacks(tenant, json, output).await,
        StackCommand::Create {
            name,
            adapters,
            description,
            workflow,
            base_url,
        } => create_stack(&name, &adapters, description, workflow, &base_url, output).await,
        StackCommand::Show {
            stack_id,
            json,
            base_url,
        } => show_stack(&stack_id, json, &base_url, output).await,
        StackCommand::Delete {
            stack_id,
            confirm,
            base_url,
        } => delete_stack(&stack_id, confirm, &base_url, output).await,
        StackCommand::Activate {
            stack_id,
            tenant,
            base_url,
        } => activate_stack(&stack_id, tenant, &base_url, output).await,
        StackCommand::Deactivate { tenant, base_url } => {
            deactivate_stack(tenant, &base_url, output).await
        }
        StackCommand::SetDefault {
            stack_id,
            tenant,
            base_url,
        } => set_default_stack(&stack_id, tenant, &base_url, output).await,
    }
}

/// Get stack command name for telemetry
fn get_stack_command_name(cmd: &StackCommand) -> String {
    match cmd {
        StackCommand::List { .. } => "stack_list",
        StackCommand::Create { .. } => "stack_create",
        StackCommand::Show { .. } => "stack_show",
        StackCommand::Delete { .. } => "stack_delete",
        StackCommand::Activate { .. } => "stack_activate",
        StackCommand::Deactivate { .. } => "stack_deactivate",
        StackCommand::SetDefault { .. } => "stack_set_default",
    }
    .to_string()
}

/// Extract tenant ID from stack command
fn extract_tenant_from_stack_command(cmd: &StackCommand) -> Option<String> {
    match cmd {
        StackCommand::List { tenant, .. } => tenant.clone(),
        StackCommand::Activate { tenant, .. } => tenant.clone(),
        StackCommand::Deactivate { tenant, .. } => tenant.clone(),
        StackCommand::SetDefault { tenant, .. } => tenant.clone(),
        _ => None,
    }
}

/// List all adapter stacks
async fn list_stacks(tenant: Option<String>, json: bool, output: &OutputWriter) -> Result<()> {
    info!("Listing adapter stacks");

    let client = reqwest::Client::new();
    let url = if let Some(ref tenant_id) = tenant {
        format!(
            "http://127.0.0.1:8080/api/v1/adapter-stacks?tenant={}",
            tenant_id
        )
    } else {
        "http://127.0.0.1:8080/api/v1/adapter-stacks".to_string()
    };

    let resp = client
        .get(&url)
        .send()
        .await
        .map_err(|e| AosError::Io(format!("HTTP request failed: {}", e)))?;

    let status = resp.status();
    if !status.is_success() {
        let text = resp.text().await.unwrap_or_default();
        return Err(AosError::Other(format!(
            "Failed to list stacks: {} {}",
            status, text
        )));
    }

    let stacks: Vec<StackResponse> = resp
        .json()
        .await
        .map_err(|e| AosError::Http(e.to_string()))?;

    if json {
        output.result(&serde_json::to_string_pretty(&stacks)?);
    } else {
        if stacks.is_empty() {
            output.info("No adapter stacks found");
            return Ok(());
        }

        let mut table = Table::new();
        table
            .load_preset(UTF8_FULL)
            .apply_modifier(UTF8_ROUND_CORNERS)
            .set_header(vec![
                "ID", "Name", "Tenant", "Adapters", "Active", "Version", "State",
            ]);

        for stack in &stacks {
            let active_indicator = if stack.is_active { "yes" } else { "no" };
            let adapter_count = stack.adapter_ids.len();

            table.add_row(vec![
                &stack.id[..8], // Short ID
                &stack.name,
                &stack.tenant_id,
                &adapter_count.to_string(),
                active_indicator,
                &stack.version.to_string(),
                &stack.lifecycle_state,
            ]);
        }

        output.result(&format!("{table}"));
    }

    Ok(())
}

/// Create a new adapter stack
async fn create_stack(
    name: &str,
    adapters: &[String],
    description: Option<String>,
    workflow: Option<String>,
    base_url: &str,
    output: &OutputWriter,
) -> Result<()> {
    info!(name = %name, adapters = ?adapters, "Creating adapter stack");

    if adapters.is_empty() {
        return Err(AosError::Validation(
            "At least one adapter must be specified".to_string(),
        ));
    }

    let workflow_type = workflow.as_deref().map(|w| match w {
        "parallel" => WorkflowType::Parallel,
        "upstream_downstream" => WorkflowType::UpstreamDownstream,
        "sequential" => WorkflowType::Sequential,
        _ => WorkflowType::Parallel, // Default
    });

    let request = CreateStackRequest {
        name: name.to_string(),
        description,
        adapter_ids: adapters.to_vec(),
        workflow_type,
        metadata: None,
    };

    let client = reqwest::Client::new();
    let url = format!("{}/v1/adapter-stacks", base_url.trim_end_matches('/'));

    let resp = client
        .post(&url)
        .json(&request)
        .send()
        .await
        .map_err(|e| AosError::Io(format!("HTTP request failed: {}", e)))?;

    let status = resp.status();
    if !status.is_success() {
        let text = resp.text().await.unwrap_or_default();
        return Err(AosError::Other(format!(
            "Failed to create stack: {} {}",
            status, text
        )));
    }

    let stack: StackResponse = resp
        .json()
        .await
        .map_err(|e| AosError::Http(e.to_string()))?;

    if output.is_json() {
        output.result(&serde_json::to_string_pretty(&stack)?);
    } else {
        output.success(&format!("Created adapter stack: {}", stack.name));
        output.kv("Stack ID", &stack.id);
        output.kv("Adapters", &stack.adapter_ids.len().to_string());
        output.kv("Version", &stack.version.to_string());

        if !stack.warnings.is_empty() {
            output.blank();
            output.warning("Warnings:");
            for warning in &stack.warnings {
                output.warning(&format!("  - {}", warning));
            }
        }
    }

    Ok(())
}

/// Show stack details
async fn show_stack(
    stack_id: &str,
    json: bool,
    base_url: &str,
    output: &OutputWriter,
) -> Result<()> {
    info!(stack_id = %stack_id, "Showing stack details");

    let client = reqwest::Client::new();
    let url = format!(
        "{}/v1/adapter-stacks/{}",
        base_url.trim_end_matches('/'),
        stack_id
    );

    let resp = client
        .get(&url)
        .send()
        .await
        .map_err(|e| AosError::Io(format!("HTTP request failed: {}", e)))?;

    let status = resp.status();
    if !status.is_success() {
        let text = resp.text().await.unwrap_or_default();
        return Err(AosError::Other(format!(
            "Failed to get stack: {} {}",
            status, text
        )));
    }

    let stack: StackResponse = resp
        .json()
        .await
        .map_err(|e| AosError::Http(e.to_string()))?;

    if json {
        output.result(&serde_json::to_string_pretty(&stack)?);
    } else {
        output.info(&format!("Stack: {}", stack.name));
        output.blank();
        output.kv("ID", &stack.id);
        output.kv("Tenant", &stack.tenant_id);
        output.kv("Description", stack.description.as_deref().unwrap_or("N/A"));
        output.kv("Active", if stack.is_active { "yes" } else { "no" });
        output.kv("Version", &stack.version.to_string());
        output.kv("Lifecycle", &stack.lifecycle_state);
        output.kv("Created", &stack.created_at);
        output.kv("Updated", &stack.updated_at);
        output.blank();
        output.info("Adapters:");
        for (i, adapter_id) in stack.adapter_ids.iter().enumerate() {
            output.result(&format!("  {}. {}", i + 1, adapter_id));
        }

        if !stack.warnings.is_empty() {
            output.blank();
            output.warning("Warnings:");
            for warning in &stack.warnings {
                output.warning(&format!("  - {}", warning));
            }
        }
    }

    Ok(())
}

/// Delete an adapter stack
async fn delete_stack(
    stack_id: &str,
    confirm: bool,
    base_url: &str,
    output: &OutputWriter,
) -> Result<()> {
    info!(stack_id = %stack_id, "Deleting adapter stack");

    if !confirm {
        output.warning("Use --confirm to delete the stack");
        return Ok(());
    }

    let client = reqwest::Client::new();
    let url = format!(
        "{}/v1/adapter-stacks/{}",
        base_url.trim_end_matches('/'),
        stack_id
    );

    let resp = client
        .delete(&url)
        .send()
        .await
        .map_err(|e| AosError::Io(format!("HTTP request failed: {}", e)))?;

    let status = resp.status();
    if !status.is_success() {
        let text = resp.text().await.unwrap_or_default();
        return Err(AosError::Other(format!(
            "Failed to delete stack: {} {}",
            status, text
        )));
    }

    output.success(&format!("Deleted adapter stack: {}", stack_id));
    Ok(())
}

/// Activate an adapter stack
async fn activate_stack(
    stack_id: &str,
    tenant: Option<String>,
    base_url: &str,
    output: &OutputWriter,
) -> Result<()> {
    info!(stack_id = %stack_id, tenant = ?tenant, "Activating adapter stack");

    let client = reqwest::Client::new();
    let url = format!(
        "{}/v1/adapter-stacks/{}/activate",
        base_url.trim_end_matches('/'),
        stack_id
    );

    let mut body = serde_json::json!({});
    if let Some(tenant_id) = tenant {
        body["tenant_id"] = serde_json::Value::String(tenant_id);
    }

    let resp = client
        .post(&url)
        .json(&body)
        .send()
        .await
        .map_err(|e| AosError::Io(format!("HTTP request failed: {}", e)))?;

    let status = resp.status();
    if !status.is_success() {
        let text = resp.text().await.unwrap_or_default();
        return Err(AosError::Other(format!(
            "Failed to activate stack: {} {}",
            status, text
        )));
    }

    output.success(&format!("Activated adapter stack: {}", stack_id));
    Ok(())
}

/// Deactivate current adapter stack
async fn deactivate_stack(
    tenant: Option<String>,
    base_url: &str,
    output: &OutputWriter,
) -> Result<()> {
    info!(tenant = ?tenant, "Deactivating adapter stack");

    let client = reqwest::Client::new();
    let url = format!(
        "{}/v1/adapter-stacks/deactivate",
        base_url.trim_end_matches('/')
    );

    let mut body = serde_json::json!({});
    if let Some(tenant_id) = tenant {
        body["tenant_id"] = serde_json::Value::String(tenant_id);
    }

    let resp = client
        .post(&url)
        .json(&body)
        .send()
        .await
        .map_err(|e| AosError::Io(format!("HTTP request failed: {}", e)))?;

    let status = resp.status();
    if !status.is_success() {
        let text = resp.text().await.unwrap_or_default();
        return Err(AosError::Other(format!(
            "Failed to deactivate stack: {} {}",
            status, text
        )));
    }

    output.success("Deactivated current adapter stack");
    Ok(())
}

/// Set stack as default for tenant
async fn set_default_stack(
    stack_id: &str,
    tenant: Option<String>,
    base_url: &str,
    output: &OutputWriter,
) -> Result<()> {
    info!(stack_id = %stack_id, tenant = ?tenant, "Setting default adapter stack");

    // Note: This endpoint may not exist yet in the API, but we'll implement the CLI command
    // for completeness. The API handler would need to be added to adapter_stacks.rs
    let client = reqwest::Client::new();
    let url = format!(
        "{}/v1/adapter-stacks/{}/set-default",
        base_url.trim_end_matches('/'),
        stack_id
    );

    let mut body = serde_json::json!({});
    if let Some(tenant_id) = tenant {
        body["tenant_id"] = serde_json::Value::String(tenant_id);
    }

    let resp = client
        .post(&url)
        .json(&body)
        .send()
        .await
        .map_err(|e| AosError::Io(format!("HTTP request failed: {}", e)))?;

    let status = resp.status();
    if !status.is_success() {
        let text = resp.text().await.unwrap_or_default();
        return Err(AosError::Other(format!(
            "Failed to set default stack: {} {}",
            status, text
        )));
    }

    output.success(&format!("Set default adapter stack: {}", stack_id));
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_get_stack_command_name() {
        assert_eq!(
            get_stack_command_name(&StackCommand::List {
                tenant: None,
                json: false
            }),
            "stack_list"
        );
        assert_eq!(
            get_stack_command_name(&StackCommand::Create {
                name: "test".to_string(),
                adapters: vec![],
                description: None,
                workflow: None,
                base_url: "http://localhost:8080".to_string(),
            }),
            "stack_create"
        );
    }

    #[test]
    fn test_extract_tenant_from_stack_command() {
        assert_eq!(
            extract_tenant_from_stack_command(&StackCommand::List {
                tenant: Some("dev".to_string()),
                json: false
            }),
            Some("dev".to_string())
        );
        assert_eq!(
            extract_tenant_from_stack_command(&StackCommand::List {
                tenant: None,
                json: false
            }),
            None
        );
    }
}
