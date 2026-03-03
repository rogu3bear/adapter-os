use adapteros_api_types::inference::{
    AdapterAttachReason, AdapterAttachment, DegradedNotice, DegradedNoticeKind, DegradedNoticeLevel,
};

pub(super) fn format_token_display(
    total: u32,
    prompt_tokens: Option<u32>,
    completion_tokens: Option<u32>,
) -> String {
    match (prompt_tokens, completion_tokens) {
        (Some(prompt), Some(completion)) => {
            format!(
                "{} tokens ({} prompt, {} completion)",
                total, prompt, completion
            )
        }
        _ => format!("{} tokens", total),
    }
}

pub(super) fn trust_summary_label(
    citation_count: usize,
    document_link_count: usize,
    adapter_attachments: &[AdapterAttachment],
    adapters_used: &[String],
    degraded_count: usize,
) -> String {
    let mut parts: Vec<String> = Vec::new();

    if citation_count > 0 {
        parts.push(format!(
            "{} source{}",
            citation_count,
            plural_suffix(citation_count)
        ));
    }

    if document_link_count > 0 {
        parts.push(format!(
            "{} document{}",
            document_link_count,
            plural_suffix(document_link_count)
        ));
    }

    if let Some(first_attachment) = adapter_attachments.first() {
        let label = first_attachment
            .adapter_label
            .clone()
            .unwrap_or_else(|| short_adapter_label(&first_attachment.adapter_id));
        let extra = adapter_attachments.len().saturating_sub(1);
        if extra > 0 {
            parts.push(format!("{label} +{extra} adapter{}", plural_suffix(extra)));
        } else {
            parts.push(label);
        }
    } else if let Some(first_adapter) = adapters_used.first() {
        let extra = adapters_used.len().saturating_sub(1);
        if extra > 0 {
            parts.push(format!(
                "{} +{} adapter{}",
                short_adapter_label(first_adapter),
                extra,
                plural_suffix(extra)
            ));
        } else {
            parts.push(short_adapter_label(first_adapter));
        }
    }

    if degraded_count > 0 {
        parts.push(format!(
            "{} degraded notice{}",
            degraded_count,
            plural_suffix(degraded_count)
        ));
    }

    if parts.is_empty() {
        "Open trust details".to_string()
    } else {
        parts.join(" · ")
    }
}

pub(super) fn plural_suffix(count: usize) -> &'static str {
    if count == 1 {
        ""
    } else {
        "s"
    }
}

pub(super) fn short_adapter_label(adapter_id: &str) -> String {
    adapter_id
        .strip_prefix("adp_")
        .or_else(|| adapter_id.strip_prefix("adp-"))
        .unwrap_or(adapter_id)
        .to_string()
}

pub(super) fn attach_reason_label(reason: &AdapterAttachReason) -> &'static str {
    match reason {
        AdapterAttachReason::Requested => "requested",
        AdapterAttachReason::Pinned => "pinned",
        AdapterAttachReason::StackRouting => "stack routing",
        AdapterAttachReason::FallbackRouting => "fallback routing",
        AdapterAttachReason::Unknown => "automatic",
    }
}

pub(super) fn attach_reason_detail(reason: &AdapterAttachReason) -> &'static str {
    match reason {
        AdapterAttachReason::Requested => "Added because you requested this adapter directly.",
        AdapterAttachReason::Pinned => "Added because this adapter is pinned in the current chat.",
        AdapterAttachReason::StackRouting => "Added by the active stack routing policy.",
        AdapterAttachReason::FallbackRouting => {
            "Added during fallback after part of the requested route degraded."
        }
        AdapterAttachReason::Unknown => "Added by automatic routing.",
    }
}

pub(super) fn degraded_kind_label(kind: &DegradedNoticeKind) -> &'static str {
    match kind {
        DegradedNoticeKind::AttachFailure => "Attach failure",
        DegradedNoticeKind::WorkerSemanticFallback => "Semantic fallback",
        DegradedNoticeKind::RoutingOverride => "Routing override",
        DegradedNoticeKind::BlockedPins => "Blocked pins",
        DegradedNoticeKind::WorkerUnavailable => "Worker unavailable",
        DegradedNoticeKind::FfiAttachFailure => "Low-level attach failure",
    }
}

pub(super) fn degraded_level_label(level: &DegradedNoticeLevel) -> &'static str {
    match level {
        DegradedNoticeLevel::Info => "info",
        DegradedNoticeLevel::Warning => "warning",
        DegradedNoticeLevel::Critical => "critical",
    }
}

pub(super) fn degraded_level_class(level: &DegradedNoticeLevel) -> &'static str {
    match level {
        DegradedNoticeLevel::Info => "border-info/30 bg-info/5",
        DegradedNoticeLevel::Warning => "border-warning/30 bg-warning/10",
        DegradedNoticeLevel::Critical => "border-destructive/40 bg-destructive/10",
    }
}

pub(super) fn prominent_degraded_title(notices: &[DegradedNotice]) -> &'static str {
    if notices
        .iter()
        .any(|notice| notice.kind == DegradedNoticeKind::FfiAttachFailure)
    {
        "Meaning changed: low-level adapter attach failed"
    } else if notices
        .iter()
        .any(|notice| notice.kind == DegradedNoticeKind::WorkerSemanticFallback)
    {
        "Meaning changed: fallback worker path used"
    } else if notices
        .iter()
        .any(|notice| notice.kind == DegradedNoticeKind::WorkerUnavailable)
    {
        "Response path failed: worker unavailable"
    } else if notices
        .iter()
        .any(|notice| notice.kind == DegradedNoticeKind::AttachFailure)
    {
        "Meaning changed: adapter attach failed"
    } else {
        "Meaning changed during execution"
    }
}

pub(super) fn citation_page_span_label(citation: &crate::signals::chat::ChatCitation) -> String {
    if let Some(page) = citation.page_number {
        format!(
            "Page {} · chars {}-{}",
            page, citation.offset_start, citation.offset_end
        )
    } else {
        format!("Chars {}-{}", citation.offset_start, citation.offset_end)
    }
}
