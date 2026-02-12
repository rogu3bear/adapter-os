//! Provenance badge component for adapter cards and detail pages.

use leptos::prelude::*;

/// Provenance verification status
#[derive(Debug, Clone, PartialEq)]
enum ProvenanceStatus {
    NoCertificate,
    Verified(f64),
    Partial(f64),
    Minimal(f64),
}

impl ProvenanceStatus {
    fn from_score(score: Option<f64>) -> Self {
        match score {
            None => ProvenanceStatus::NoCertificate,
            Some(s) if s > 0.8 => ProvenanceStatus::Verified(s),
            Some(s) if s >= 0.4 => ProvenanceStatus::Partial(s),
            Some(s) => ProvenanceStatus::Minimal(s),
        }
    }

    fn css_class(&self) -> &'static str {
        match self {
            ProvenanceStatus::NoCertificate => "provenance-badge provenance-none",
            ProvenanceStatus::Verified(_) => "provenance-badge provenance-verified",
            ProvenanceStatus::Partial(_) => "provenance-badge provenance-partial",
            ProvenanceStatus::Minimal(_) => "provenance-badge provenance-minimal",
        }
    }

    fn label(&self) -> String {
        match self {
            ProvenanceStatus::NoCertificate => "No cert".to_string(),
            ProvenanceStatus::Verified(score) => format!("Verified {:.0}%", score * 100.0),
            ProvenanceStatus::Partial(score) => format!("Partial {:.0}%", score * 100.0),
            ProvenanceStatus::Minimal(score) => format!("Minimal {:.0}%", score * 100.0),
        }
    }

    fn icon(&self) -> &'static str {
        match self {
            ProvenanceStatus::Verified(_) => "\u{1f6e1}\u{fe0f}",
            ProvenanceStatus::Partial(_) => "\u{26a0}\u{fe0f}",
            ProvenanceStatus::Minimal(_) | ProvenanceStatus::NoCertificate => "\u{25cb}",
        }
    }
}

/// Compact provenance badge for adapter cards and detail headers.
///
/// Displays a shield icon with completeness status based on an optional
/// provenance score. The score is derived from the adapter's provenance
/// certificate (if one exists).
#[component]
pub fn ProvenanceBadge(
    /// Optional completeness score (0.0–1.0) from the adapter's provenance certificate.
    #[prop(optional)]
    score: Option<f64>,
) -> impl IntoView {
    let status = ProvenanceStatus::from_score(score);
    let css = status.css_class();
    let icon = status.icon();
    let label = status.label();

    view! {
        <span class=css title="Provenance certificate status">
            <span class="provenance-icon">{icon}</span>
            <span class="provenance-label">{label}</span>
        </span>
    }
}
