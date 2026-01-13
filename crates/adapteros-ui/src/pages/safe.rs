//! Safe mode page (no auth, no API calls).

use crate::components::{Button, Card};
use leptos::prelude::*;

/// Safe mode page.
#[component]
pub fn Safe() -> impl IntoView {
    let go_login = move |_| {
        if let Some(window) = web_sys::window() {
            let _ = window.location().set_href("/login");
        }
    };

    view! {
        <div class="min-h-screen flex items-center justify-center bg-muted/40 p-6">
            <Card
                title="Safe Mode".to_string()
                description="Minimal UI with no API calls. Use this if the main app fails to load.".to_string()
                class="w-full max-w-lg".to_string()
            >
                <div class="space-y-4">
                    <p class="text-sm text-muted-foreground">
                        "Safe mode helps diagnose boot issues and provides a stable fallback UI."
                    </p>
                    <div class="flex items-center gap-3">
                        <Button on_click=Callback::new(go_login)>
                            "Go to Login"
                        </Button>
                        <Button
                            variant=crate::components::ButtonVariant::Outline
                            on_click=Callback::new(move |_| {
                                if let Some(window) = web_sys::window() {
                                    let _ = window.location().set_href("/");
                                }
                            })
                        >
                            "Try Main App"
                        </Button>
                    </div>
                </div>
            </Card>
        </div>
    }
}
