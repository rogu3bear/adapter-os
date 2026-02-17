//! Version history timeline component.
//!
//! Displays a vertical timeline of version history events for an adapter repository.
//! Fetches from `GET /v1/repos/{repo_id}/timeline` and renders each event with
//! timestamp, event type badge, and description.
//!
//! ## Design
//!
//! Uses Liquid Glass Tier 2 cards for timeline items with a vertical connecting line.

use leptos::prelude::*;
use leptos::task::spawn_local;
use std::sync::Arc;

use crate::api::types::TimelineEvent;
use crate::api::use_api_client;
use crate::components::{Badge, BadgeVariant, Card, Spinner};

fn read_query_param(name: &str) -> Option<String> {
    #[cfg(target_arch = "wasm32")]
    {
        let window = web_sys::window()?;
        let location = window.location();
        let search = location.search().ok()?;
        let query = search.strip_prefix('?').unwrap_or(&search);
        for pair in query.split('&') {
            if pair.is_empty() {
                continue;
            }
            if let Some((key, value)) = pair.split_once('=') {
                if key == name {
                    return Some(value.to_string());
                }
            } else if pair == name {
                return Some(String::new());
            }
        }
        None
    }
    #[cfg(not(target_arch = "wasm32"))]
    {
        let _ = name;
        None
    }
}

/// Classify a timeline event type string into a display label and badge variant.
fn classify_event(event_type: &str) -> (&'static str, BadgeVariant) {
    // event_type format is "state_change:<new_state>" from the server handler
    let lower = event_type.to_lowercase();
    if lower.contains("promoted") || lower.contains("promotion") {
        ("Promotion", BadgeVariant::Success)
    } else if lower.contains("rollback") {
        ("Rollback", BadgeVariant::Destructive)
    } else if lower.contains("retired") {
        ("Retired", BadgeVariant::Destructive)
    } else if lower.contains("deprecated") {
        ("Deprecated", BadgeVariant::Warning)
    } else if lower.contains("active") {
        ("Active", BadgeVariant::Success)
    } else if lower.contains("draft") {
        ("Draft", BadgeVariant::Secondary)
    } else if lower.contains("candidate") {
        ("Candidate", BadgeVariant::Warning)
    } else if lower.contains("state_change") {
        ("State Change", BadgeVariant::Default)
    } else {
        ("Event", BadgeVariant::Default)
    }
}

/// Format an ISO 8601 timestamp as a relative time string (e.g. "2 hours ago").
///
/// Falls back to the raw timestamp if parsing fails. Uses JS `Date` via `web_sys`
/// for correct timezone handling in WASM.
fn format_relative_time(timestamp: &str) -> String {
    use js_sys::Date;

    let event_ms = Date::parse(timestamp);
    if event_ms.is_nan() {
        return timestamp.to_string();
    }

    let now_ms = Date::now();
    let diff_secs = ((now_ms - event_ms) / 1000.0) as i64;

    if diff_secs < 0 {
        return "just now".to_string();
    }

    match diff_secs {
        0..=59 => "just now".to_string(),
        60..=3599 => {
            let mins = diff_secs / 60;
            if mins == 1 {
                "1 minute ago".to_string()
            } else {
                format!("{} minutes ago", mins)
            }
        }
        3600..=86399 => {
            let hours = diff_secs / 3600;
            if hours == 1 {
                "1 hour ago".to_string()
            } else {
                format!("{} hours ago", hours)
            }
        }
        86400..=2591999 => {
            let days = diff_secs / 86400;
            if days == 1 {
                "1 day ago".to_string()
            } else {
                format!("{} days ago", days)
            }
        }
        _ => {
            let days = diff_secs / 86400;
            if days < 365 {
                let months = days / 30;
                if months <= 1 {
                    "1 month ago".to_string()
                } else {
                    format!("{} months ago", months)
                }
            } else {
                let years = days / 365;
                if years == 1 {
                    "1 year ago".to_string()
                } else {
                    format!("{} years ago", years)
                }
            }
        }
    }
}

/// Version history timeline component.
///
/// Fetches and displays version history events for a repository in a vertical
/// timeline layout. Each event shows its timestamp, event type badge, and
/// description text.
#[component]
pub fn VersionTimeline(
    /// Repository ID to fetch timeline for
    #[prop(into)]
    repo_id: String,
) -> impl IntoView {
    let client = use_api_client();
    let events = RwSignal::new(Vec::<TimelineEvent>::new());
    let loading = RwSignal::new(true);
    let error = RwSignal::new(None::<String>);
    let focused_event_id = Signal::derive(move || read_query_param("timeline_event_id"));

    // Fetch timeline on mount
    {
        let client = Arc::clone(&client);
        let repo_id = repo_id.clone();
        spawn_local(async move {
            match client.get_repo_timeline(&repo_id).await {
                Ok(timeline) => {
                    events.set(timeline);
                    loading.set(false);
                }
                Err(e) => {
                    error.set(Some(e.to_string()));
                    loading.set(false);
                }
            }
        });
    }

    Effect::new(move || {
        if loading.try_get().unwrap_or(true) {
            return;
        }
        let _ = events.try_get();
        let Some(focus_id) = focused_event_id.try_get().flatten() else {
            return;
        };
        #[cfg(target_arch = "wasm32")]
        {
            if let Some(window) = web_sys::window() {
                if let Some(document) = window.document() {
                    if let Some(element) =
                        document.get_element_by_id(&format!("timeline-event-{}", focus_id))
                    {
                        element.scroll_into_view();
                    }
                }
            }
        }
    });

    view! {
        <Card title="Version History">
            {move || {
                if loading.try_get().unwrap_or(true) {
                    return view! {
                        <div class="version-timeline-loading">
                            <Spinner />
                            <span class="version-timeline-loading-text">"Loading history..."</span>
                        </div>
                    }.into_any();
                }

                if let Some(err) = error.try_get().flatten() {
                    return view! {
                        <p class="version-timeline-error">{format!("Could not load timeline: {}", err)}</p>
                    }.into_any();
                }

                let items = events.try_get().unwrap_or_default();
                if items.is_empty() {
                    return view! {
                        <p class="version-timeline-empty">"No version history yet"</p>
                    }.into_any();
                }

                view! {
                    <div class="version-timeline">
                        {items.into_iter().map(|event| {
                            let is_focused = focused_event_id
                                .try_get_untracked()
                                .flatten()
                                .as_deref()
                                == Some(event.id.as_str());
                            let (label, variant) = classify_event(&event.event_type);
                            let relative = format_relative_time(&event.timestamp);
                            let raw_timestamp = event.timestamp.clone();
                            let event_dom_id = format!("timeline-event-{}", event.id);

                            view! {
                                <div
                                    id=event_dom_id
                                    class="version-timeline-item"
                                    style=if is_focused {
                                        "box-shadow: inset 0 0 0 1px rgba(14, 165, 233, 0.7); background-color: rgba(14, 165, 233, 0.08);"
                                    } else {
                                        ""
                                    }
                                >
                                    <div class="version-timeline-dot-column">
                                        <div class="version-timeline-dot"></div>
                                        <div class="version-timeline-line"></div>
                                    </div>
                                    <div class="version-timeline-content">
                                        <div class="version-timeline-header">
                                            <Badge variant=variant>{label}</Badge>
                                            <span class="version-timeline-time" title=raw_timestamp>{relative}</span>
                                        </div>
                                        <p class="version-timeline-description">{event.description}</p>
                                    </div>
                                </div>
                            }
                        }).collect_view()}
                    </div>
                }.into_any()
            }}
        </Card>
    }
}
