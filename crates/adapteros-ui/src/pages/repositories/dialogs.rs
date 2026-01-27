//! Repository dialog components

use crate::api::{ApiClient, RegisterRepositoryRequest};
use crate::components::Input;
use crate::signals::use_auth;
use leptos::prelude::*;

/// Register repository dialog
#[component]
pub fn RegisterRepositoryDialog(open: RwSignal<bool>) -> impl IntoView {
    let (auth_state, _) = use_auth();
    // Form state
    let repo_id = RwSignal::new(String::new());
    let path = RwSignal::new(String::new());
    let languages = RwSignal::new(String::new());
    let default_branch = RwSignal::new("main".to_string());

    let submitting = RwSignal::new(false);
    let error = RwSignal::new(None::<String>);

    view! {
        <Show when=move || open.get() fallback=|| view! {}>
            // Backdrop
            <div
                class="fixed inset-0 z-50 bg-black/80"
                on:click=move |_| {
                    open.set(false);
                    error.set(None);
                }
            />

            // Dialog
            <div class="dialog-content">
                // Header
                <div class="flex items-center justify-between mb-4">
                    <div>
                        <h2 class="text-lg font-semibold">"Register Repository"</h2>
                        <p class="text-sm text-muted-foreground">"Add a codebase for adapter training"</p>
                    </div>
                    <button
                        class="rounded-sm opacity-70 hover:opacity-100 focus:outline-none focus:ring-2 focus:ring-ring focus:ring-offset-2"
                        aria-label="Close"
                        type="button"
                        on:click=move |_| {
                            open.set(false);
                            error.set(None);
                        }
                    >
                        <svg
                            xmlns="http://www.w3.org/2000/svg"
                            width="24"
                            height="24"
                            viewBox="0 0 24 24"
                            fill="none"
                            stroke="currentColor"
                            stroke-width="2"
                        >
                            <path d="M18 6 6 18"/>
                            <path d="m6 6 12 12"/>
                        </svg>
                    </button>
                </div>

                // Error message
                {move || error.get().map(|e| view! {
                    <div class="mb-4 rounded-lg border border-destructive bg-destructive/10 p-3">
                        <p class="text-sm text-destructive">{e}</p>
                    </div>
                })}

                // Form
                <div class="space-y-4">
                    <Input
                        value=repo_id
                        label="Repository ID".to_string()
                        placeholder="my-project".to_string()
                    />
                    <Input
                        value=path
                        label="Path".to_string()
                        placeholder="/path/to/repository".to_string()
                    />
                    <Input
                        value=languages
                        label="Languages (comma-separated)".to_string()
                        placeholder="rust, python, typescript".to_string()
                    />
                    <Input
                        value=default_branch
                        label="Default Branch".to_string()
                        placeholder="main".to_string()
                    />
                </div>

                // Footer
                <div class="flex justify-end gap-2 mt-6">
                    <button
                        class="inline-flex items-center gap-2 rounded-md border border-input bg-background px-4 py-2 text-sm font-medium hover:bg-accent focus:outline-none focus:ring-2 focus:ring-ring focus:ring-offset-2"
                        on:click=move |_| {
                            open.set(false);
                            error.set(None);
                        }
                    >
                        "Cancel"
                    </button>
                    <button
                        class="inline-flex items-center gap-2 rounded-md bg-primary px-4 py-2 text-sm font-medium text-primary-foreground hover:bg-primary/90 disabled:opacity-50 focus:outline-none focus:ring-2 focus:ring-ring focus:ring-offset-2"
                        disabled=submitting.get()
                        on:click=move |_| {
                            // Validate
                            let rid = repo_id.get();
                            let p = path.get();

                            if rid.is_empty() {
                                error.set(Some("Repository ID is required".to_string()));
                                return;
                            }
                            if p.is_empty() {
                                error.set(Some("Path is required".to_string()));
                                return;
                            }

                            error.set(None);
                            submitting.set(true);

                            let Some(user) = auth_state.get().user().cloned() else {
                                error.set(Some("Authentication required to register repository".to_string()));
                                submitting.set(false);
                                return;
                            };

                            let langs: Vec<String> = languages
                                .get()
                                .split(',')
                                .map(|s| s.trim().to_string())
                                .filter(|s| !s.is_empty())
                                .collect();
                            let branch = default_branch.get();

                            wasm_bindgen_futures::spawn_local(async move {
                                let client = ApiClient::new();

                                let request = RegisterRepositoryRequest {
                                    tenant_id: user.tenant_id,
                                    repo_id: rid,
                                    path: p,
                                    languages: langs,
                                    default_branch: branch,
                                };

                                match client.register_repository(&request).await {
                                    Ok(_) => {
                                        submitting.set(false);
                                        // Reset form
                                        repo_id.set(String::new());
                                        path.set(String::new());
                                        languages.set(String::new());
                                        default_branch.set("main".to_string());
                                        open.set(false);
                                    }
                                    Err(e) => {
                                        error.set(Some(e.to_string()));
                                        submitting.set(false);
                                    }
                                }
                            });
                        }
                    >
                        {move || if submitting.get() { "Registering..." } else { "Register" }}
                    </button>
                </div>
            </div>
        </Show>
    }
}
