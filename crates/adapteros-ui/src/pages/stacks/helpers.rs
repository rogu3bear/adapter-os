//! Helper functions for stacks
//!
//! Shared utilities for stack components.

use crate::api::WorkflowType;
use crate::components::BadgeVariant;

pub fn workflow_type_label(wf: &Option<WorkflowType>) -> &'static str {
    match wf {
        Some(WorkflowType::Parallel) => "Parallel",
        Some(WorkflowType::Sequential) => "Sequential",
        Some(WorkflowType::UpstreamDownstream) => "Upstream/Downstream",
        None => "Default",
    }
}

pub fn lifecycle_badge_variant(state: &str) -> BadgeVariant {
    match state {
        "active" => BadgeVariant::Success,
        "deprecated" => BadgeVariant::Warning,
        "retired" => BadgeVariant::Destructive,
        "draft" => BadgeVariant::Secondary,
        _ => BadgeVariant::Secondary,
    }
}
