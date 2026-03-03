use leptos::prelude::*;

#[derive(Clone, Debug)]
pub struct ReadinessCheckItem {
    pub label: String,
    pub hint: Option<String>,
    pub status_class: &'static str,
    pub action_href: Option<String>,
    pub action_label: Option<String>,
}

/// Shared checklist renderer for onboarding readiness checks.
#[component]
pub fn OnboardingReadinessChecklist(items: Vec<ReadinessCheckItem>) -> impl IntoView {
    view! {
        <ul class="welcome-checks">
            {items
                .into_iter()
                .map(|item| {
                    let row_class = format!("welcome-check-item {}", item.status_class);
                    view! {
                        <li class=row_class>
                            <span class="welcome-check-icon" aria-hidden="true"></span>
                            <div class="welcome-check-content">
                                <span class="welcome-check-label">{item.label}</span>
                                {item
                                    .hint
                                    .map(|hint| view! { <span class="welcome-check-hint">{hint}</span> })}
                            </div>
                            {match (item.action_href, item.action_label) {
                                (Some(href), Some(label)) => Some(
                                    view! {
                                        <a class="welcome-check-action" href=href>
                                            {label}
                                        </a>
                                    },
                                ),
                                _ => None,
                            }}
                        </li>
                    }
                })
                .collect_view()}
        </ul>
    }
}
