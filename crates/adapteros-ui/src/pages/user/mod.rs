//! User page
//!
//! Personal user hub that brings profile, preferences, and API config together.

use crate::pages::settings::{
    SettingsApiConfigSection, SettingsPreferencesSection, SettingsProfileSection,
};
use leptos::prelude::*;

/// User page
#[component]
pub fn User() -> impl IntoView {
    view! {
        <div class="p-6 space-y-6">
            <div class="flex items-center justify-between">
                <div>
                    <h1 class="text-3xl font-bold tracking-tight">"User"</h1>
                    <p class="text-sm text-muted-foreground">
                        "Profile, personalization, and API preferences."
                    </p>
                </div>
                <div class="flex items-center gap-2 rounded-full border border-border px-3 py-1 text-xs text-muted-foreground">
                    <span class="inline-flex h-2 w-2 rounded-full bg-emerald-500"></span>
                    "Settings auto-save"
                </div>
            </div>

            <div class="grid grid-cols-1 gap-6 lg:grid-cols-3">
                <div class="lg:col-span-2 space-y-6">
                    <section class="rounded-xl border border-border bg-card/60 p-5 shadow-sm">
                        <div class="mb-4">
                            <h2 class="text-lg font-semibold">"Profile"</h2>
                            <p class="text-xs text-muted-foreground">
                                "Account identity, role, and session context."
                            </p>
                        </div>
                        <SettingsProfileSection/>
                    </section>

                    <section class="rounded-xl border border-border bg-card/60 p-5 shadow-sm">
                        <div class="mb-4">
                            <h2 class="text-lg font-semibold">"Personalization"</h2>
                            <p class="text-xs text-muted-foreground">
                                "Theme, density, timestamps, and default landing page."
                            </p>
                        </div>
                        <SettingsPreferencesSection/>
                    </section>
                </div>

                <div class="space-y-6">
                    <section class="rounded-xl border border-border bg-card/60 p-5 shadow-sm">
                        <div class="mb-4">
                            <h2 class="text-lg font-semibold">"API"</h2>
                            <p class="text-xs text-muted-foreground">
                                "Endpoint overrides and auth status."
                            </p>
                        </div>
                        <SettingsApiConfigSection/>
                    </section>

                    <section class="rounded-xl border border-border bg-gradient-to-br from-muted/50 to-background p-5">
                        <h3 class="text-sm font-semibold">"Scope"</h3>
                        <ul class="mt-2 space-y-2 text-xs text-muted-foreground">
                            <li>"Preferences are stored in this browser only."</li>
                            <li>"Profile fields are read-only today."</li>
                            <li>"API changes apply immediately."</li>
                        </ul>
                    </section>
                </div>
            </div>
        </div>
    }
}
