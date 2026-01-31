//! Login page

use crate::components::{Button, Card, FormField, Input, OfflineBanner};
use crate::signals::use_auth;
use crate::validation::{use_form_errors, validate_field, ValidationRule};
use leptos::prelude::*;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

/// Login page
#[component]
pub fn Login() -> impl IntoView {
    let username = RwSignal::new(String::new());
    let password = RwSignal::new(String::new());
    let error = RwSignal::new(Option::<String>::None);
    let loading = RwSignal::new(false);
    let errors = use_form_errors();

    // Track mounted state to prevent setting signals after unmount
    let is_mounted = Arc::new(AtomicBool::new(true));
    let is_mounted_cleanup = Arc::clone(&is_mounted);
    on_cleanup(move || {
        is_mounted_cleanup.store(false, Ordering::SeqCst);
    });

    // Validation rules
    let username_rules = vec![ValidationRule::Required, ValidationRule::MinLength(1)];
    let password_rules = vec![ValidationRule::Required, ValidationRule::MinLength(1)];

    // Derived signals for field errors
    let username_error = Signal::derive(move || errors.get().get("username").cloned());
    let password_error = Signal::derive(move || errors.get().get("password").cloned());

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
        let username_rules = username_rules.clone();
        let password_rules = password_rules.clone();
        let is_mounted = Arc::clone(&is_mounted);
        move || {
            let username_val = username.get();
            let password_val = password.get();

            // Clear previous errors (both field and API errors)
            errors.update(|e| e.clear_all());
            error.set(None);

            // Validate fields
            let mut has_errors = false;

            if let Some(err) = validate_field(&username_val, &username_rules) {
                errors.update(|e| e.set("username", err));
                has_errors = true;
            }

            if let Some(err) = validate_field(&password_val, &password_rules) {
                errors.update(|e| e.set("password", err));
                has_errors = true;
            }

            // Don't proceed if validation failed
            if has_errors {
                return;
            }

            let action = Arc::clone(&auth_action);
            let is_mounted = Arc::clone(&is_mounted);

            loading.set(true);

            wasm_bindgen_futures::spawn_local(async move {
                match action.login(&username_val, &password_val).await {
                    Ok(_) => {
                        // Navigate to dashboard
                        if let Some(window) = web_sys::window() {
                            let _ = window.location().set_href("/dashboard");
                        }
                    }
                    Err(e) => {
                        // Only update signals if component is still mounted
                        if is_mounted.load(Ordering::SeqCst) {
                            error.set(Some(e.to_string()));
                            loading.set(false);
                        }
                    }
                }
            });
        }
    };

    view! {
        <div class="min-h-screen bg-muted/40">
            // Show backend status at top of login page
            <OfflineBanner/>
            <div class="flex min-h-screen items-center justify-center">
            <Card
                title="Login".to_string()
                description="Enter your credentials to access adapterOS".to_string()
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
                    <FormField
                        label="Username"
                        name="username"
                        required=true
                        error=username_error
                    >
                        <Input
                            value=username
                            placeholder="Enter your username".to_string()
                        />
                    </FormField>

                    <FormField
                        label="Password"
                        name="password"
                        required=true
                        error=password_error
                    >
                        <Input
                            value=password
                            placeholder="Enter your password".to_string()
                            input_type="password"
                        />
                    </FormField>

                    {move || {
                        error.get().map(|e| view! {
                            <div class="rounded-md border border-destructive bg-destructive/10 p-3 text-sm text-destructive">
                                {e}
                            </div>
                        })
                    }}

                    <Button
                        class="w-full".to_string()
                        loading=loading
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
        </div>
    }
}
