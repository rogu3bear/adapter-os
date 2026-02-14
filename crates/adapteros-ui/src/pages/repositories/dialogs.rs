//! Repository dialog components
//!
//! Uses canonical Dialog component for ARIA compliance and keyboard handling.

use crate::api::{ApiClient, RegisterRepositoryRequest};
use crate::components::{Button, ButtonVariant, Dialog, FormField, Input};
use crate::signals::{use_auth, use_notifications};
use leptos::prelude::*;

/// Register repository dialog
#[component]
pub fn RegisterRepositoryDialog(open: RwSignal<bool>) -> impl IntoView {
    let (auth_state, _) = use_auth();
    let notifications = use_notifications();
    // Form state
    let repo_id = RwSignal::new(String::new());
    let path = RwSignal::new(String::new());
    let languages = RwSignal::new(String::new());
    let default_branch = RwSignal::new("main".to_string());

    let submitting = RwSignal::new(false);
    let error = RwSignal::new(None::<String>);

    let on_submit = move |_| {
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
            error.set(Some(
                "Authentication required to register repository".to_string(),
            ));
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
        let notifications = notifications.clone();

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
                    notifications.success(
                        "Repository registered",
                        "Repository registered successfully.",
                    );
                }
                Err(e) => {
                    error.set(Some(e.user_message()));
                    submitting.set(false);
                }
            }
        });
    };

    view! {
        <Dialog
            open=open
            title="Register Repository"
            description="Add a codebase for adapter training"
        >
            // Error message
            {move || error.get().map(|e| view! {
                <div class="mb-4 rounded-md border border-destructive bg-destructive/10 p-3 text-sm text-destructive">
                    {e}
                </div>
            })}

            // Form
            <div class="space-y-4">
                <FormField label="Repository ID" name="repo_id">
                    <Input
                        value=repo_id
                        placeholder="my-project".to_string()
                    />
                </FormField>
                <FormField label="Path" name="path">
                    <Input
                        value=path
                        placeholder="/path/to/repository".to_string()
                    />
                </FormField>
                <FormField label="Languages (comma-separated)" name="languages">
                    <Input
                        value=languages
                        placeholder="rust, python, typescript".to_string()
                    />
                </FormField>
                <FormField label="Default Branch" name="default_branch">
                    <Input
                        value=default_branch
                        placeholder="main".to_string()
                    />
                </FormField>
            </div>

            // Footer
            <div class="flex justify-end gap-2 mt-6">
                <Button
                    variant=ButtonVariant::Outline
                    on_click=Callback::new(move |_| {
                        open.set(false);
                        error.set(None);
                    })
                >
                    "Cancel"
                </Button>
                <Button
                    variant=ButtonVariant::Primary
                    loading=Signal::from(submitting)
                    disabled=Signal::from(submitting)
                    on_click=Callback::new(on_submit)
                >
                    "Register"
                </Button>
            </div>
        </Dialog>
    }
}
