//! Login page

use crate::components::{Button, Card, Input};
use crate::signals::use_auth;
use leptos::prelude::*;
use std::sync::Arc;

/// Login page
#[component]
pub fn Login() -> impl IntoView {

    let username = RwSignal::new(String::new());
    let password = RwSignal::new(String::new());
    let error = RwSignal::new(Option::<String>::None);
    let loading = RwSignal::new(false);

    let (auth_state, auth_action) = use_auth();
    let auth_action = Arc::new(auth_action);

    // Redirect if already authenticated
    Effect::new(move || {
        if auth_state.get().is_authenticated() {
            // Navigate to dashboard
            if let Some(window) = web_sys::window() {
                let _ = window.location().set_href("/dashboard");
            }
        }
    });

    let do_login = {
        let auth_action = Arc::clone(&auth_action);
        move || {
            let username_val = username.get();
            let password_val = password.get();
            let action = Arc::clone(&auth_action);

            loading.set(true);
            error.set(None);

            wasm_bindgen_futures::spawn_local(async move {
                match action.login(&username_val, &password_val).await {
                    Ok(_) => {
                        // Navigate to dashboard
                        if let Some(window) = web_sys::window() {
                            let _ = window.location().set_href("/dashboard");
                        }
                    }
                    Err(e) => {
                        error.set(Some(e.to_string()));
                        loading.set(false);
                    }
                }
            });
        }
    };

    view! {
        <div class="flex min-h-screen items-center justify-center bg-muted/40">
            <Card
                title="Login".to_string()
                description="Enter your credentials to access AdapterOS".to_string()
                class="w-full max-w-md".to_string()
            >
                <form
                    class="space-y-4"
                    on:submit={
                        let do_login = do_login.clone();
                        move |ev| {
                            ev.prevent_default();
                            do_login();
                        }
                    }
                >
                    <Input
                        value=username
                        label="Username".to_string()
                        placeholder="Enter your username".to_string()
                    />

                    <Input
                        value=password
                        label="Password".to_string()
                        placeholder="Enter your password".to_string()
                        input_type="password"
                    />

                    {move || {
                        error.get().map(|e| view! {
                            <div class="rounded-md bg-destructive/10 p-3 text-sm text-destructive">
                                {e}
                            </div>
                        })
                    }}

                    <Button
                        class="w-full".to_string()
                        loading=loading.get()
                        on_click=Callback::new({
                            let do_login = do_login.clone();
                            move |_| do_login()
                        })
                    >
                        "Sign In"
                    </Button>
                </form>
            </Card>
        </div>
    }
}
