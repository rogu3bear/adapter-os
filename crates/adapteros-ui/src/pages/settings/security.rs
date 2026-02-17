//! Security settings section
//!
//! Active sessions management and MFA enrollment/management.

use crate::api::{report_error_with_toast, use_api_client, ApiClient};
use crate::api::{
    MfaDisableRequest, MfaEnrollStartResponse, MfaEnrollVerifyRequest, MfaStatusResponse,
    SessionInfo, SessionsResponse,
};
use crate::components::{
    Badge, BadgeVariant, Button, ButtonVariant, Card, Dialog, ErrorDisplay, SimpleConfirmDialog,
    Spinner, Table, TableBody, TableCell, TableHead, TableHeader, TableRow,
};
use crate::hooks::{use_api_resource, LoadingState, Refetch};
use leptos::prelude::*;
use std::sync::Arc;

/// Security settings section with session management and MFA
#[component]
pub fn SecuritySection() -> impl IntoView {
    view! {
        <div class="space-y-6 max-w-2xl">
            <SessionsCard/>
            <MfaCard/>
        </div>
    }
}

// ============================================================================
// Active Sessions
// ============================================================================

/// Active sessions management card
#[component]
fn SessionsCard() -> impl IntoView {
    let (sessions, refetch) =
        use_api_resource(|client: Arc<ApiClient>| async move { client.list_auth_sessions().await });

    let revoking = RwSignal::new(Option::<String>::None);

    view! {
        <Card
            title="Active Sessions".to_string()
            description="Manage your active login sessions across devices.".to_string()
        >
            {move || {
                match sessions.try_get().unwrap_or(LoadingState::Loading) {
                    LoadingState::Idle | LoadingState::Loading => {
                        view! {
                            <div class="flex items-center gap-2">
                                <Spinner/>
                                <span class="text-sm text-muted-foreground">"Loading sessions\u{2026}"</span>
                            </div>
                        }.into_any()
                    }
                    LoadingState::Loaded(data) => {
                        let refetch = refetch;
                        view! {
                            <SessionsTable sessions=data revoking=revoking refetch=refetch/>
                        }.into_any()
                    }
                    LoadingState::Error(e) => {
                        let refetch = refetch;
                        view! {
                            <ErrorDisplay error=e on_retry=Callback::new(move |_| refetch.run(()))/>
                        }.into_any()
                    }
                }
            }}
        </Card>
    }
}

/// Table of active sessions with revoke actions
#[component]
fn SessionsTable(
    sessions: SessionsResponse,
    revoking: RwSignal<Option<String>>,
    refetch: Refetch,
) -> impl IntoView {
    let items = sessions.sessions;
    let count = items.len();

    view! {
        <div class="space-y-3">
            <div class="flex items-center justify-between">
                <span class="text-xs text-muted-foreground">
                    {format!("{} active session{}", count, if count == 1 { "" } else { "s" })}
                </span>
                {(count > 1).then(|| {
                    view! {
                        <RevokeAllButton refetch=refetch/>
                    }
                })}
            </div>

            {if items.is_empty() {
                view! {
                    <p class="text-sm text-muted-foreground">"No active sessions found."</p>
                }.into_any()
            } else {
                view! {
                    <Table>
                        <TableHeader>
                            <TableRow>
                                <TableHead>"Session ID"</TableHead>
                                <TableHead>"IP Address"</TableHead>
                                <TableHead>"User Agent"</TableHead>
                                <TableHead>"Created"</TableHead>
                                <TableHead>"Last Activity"</TableHead>
                                <TableHead>""</TableHead>
                            </TableRow>
                        </TableHeader>
                        <TableBody>
                            {items.into_iter().map(|session| {
                                    view! {
                                    <SessionRow session=session revoking=revoking refetch=refetch/>
                                }
                            }).collect_view()}
                        </TableBody>
                    </Table>
                }.into_any()
            }}
        </div>
    }
}

/// Single session row with revoke button
#[component]
fn SessionRow(
    session: SessionInfo,
    revoking: RwSignal<Option<String>>,
    refetch: Refetch,
) -> impl IntoView {
    let jti = session.jti.clone();
    let jti_display = if jti.chars().count() > 12 {
        let truncated: String = jti.chars().take(12).collect();
        format!("{truncated}\u{2026}")
    } else {
        jti.clone()
    };

    let ip = session.ip_address.unwrap_or_else(|| "\u{2014}".to_string());
    let ua = session
        .user_agent
        .map(|ua| truncate_ua(&ua))
        .unwrap_or_else(|| "\u{2014}".to_string());
    let created = format_compact_ts(&session.created_at);
    let last_active = format_compact_ts(&session.last_activity);

    let client = use_api_client();

    let is_revoking = {
        let jti = jti.clone();
        Signal::derive(move || revoking.try_get().flatten().as_deref() == Some(&jti))
    };

    let handle_revoke = {
        let jti = jti.clone();
        let client = client.clone();
        Callback::new(move |_| {
            let jti = jti.clone();
            revoking.set(Some(jti.clone()));
            let client = client.clone();
            wasm_bindgen_futures::spawn_local(async move {
                match client.revoke_auth_session(&jti).await {
                    Ok(_) => {
                        revoking.set(None);
                        refetch.run(());
                    }
                    Err(e) => {
                        revoking.set(None);
                        report_error_with_toast(
                            &e,
                            "Failed to revoke session",
                            Some("settings"),
                            true,
                        );
                    }
                }
            });
        })
    };

    view! {
        <TableRow>
            <TableCell>
                <span class="font-mono text-xs" title=jti>{jti_display}</span>
            </TableCell>
            <TableCell>
                <span class="font-mono text-xs">{ip}</span>
            </TableCell>
            <TableCell>
                <span class="text-xs truncate max-w-[200px] inline-block">{ua}</span>
            </TableCell>
            <TableCell>
                <span class="text-xs">{created}</span>
            </TableCell>
            <TableCell>
                <span class="text-xs">{last_active}</span>
            </TableCell>
            <TableCell>
                <Button
                    variant=ButtonVariant::Destructive
                    size=crate::components::ButtonSize::Sm
                    loading=is_revoking
                    on_click=handle_revoke
                >
                    "Revoke"
                </Button>
            </TableCell>
        </TableRow>
    }
}

/// "Revoke All Others" button
#[component]
fn RevokeAllButton(refetch: Refetch) -> impl IntoView {
    let show_confirm = RwSignal::new(false);
    let loading = RwSignal::new(false);
    let client = use_api_client();

    let handle_revoke_all = {
        let client = client.clone();
        Callback::new(move |_| {
            loading.set(true);
            let client = client.clone();
            wasm_bindgen_futures::spawn_local(async move {
                // Fetch current sessions, then revoke all except the first (current)
                match client.list_auth_sessions().await {
                    Ok(resp) => {
                        let mut had_error = false;
                        for (i, session) in resp.sessions.iter().enumerate() {
                            // Skip first session (likely current)
                            if i == 0 {
                                continue;
                            }
                            if let Err(e) = client.revoke_auth_session(&session.jti).await {
                                report_error_with_toast(
                                    &e,
                                    "Failed to revoke session",
                                    Some("settings"),
                                    true,
                                );
                                had_error = true;
                                break;
                            }
                        }
                        let _ = loading.try_set(false);
                        let _ = show_confirm.try_set(false);
                        if !had_error {
                            refetch.run(());
                        }
                    }
                    Err(e) => {
                        let _ = loading.try_set(false);
                        let _ = show_confirm.try_set(false);
                        report_error_with_toast(
                            &e,
                            "Failed to list sessions",
                            Some("settings"),
                            true,
                        );
                    }
                }
            });
        })
    };

    view! {
        <Button
            variant=ButtonVariant::Outline
            size=crate::components::ButtonSize::Sm
            on_click=Callback::new(move |_| show_confirm.set(true))
        >
            "Revoke All Others"
        </Button>
        <SimpleConfirmDialog
            open=show_confirm
            title="Revoke All Other Sessions"
            description="This will sign out all other devices. Your current session will remain active."
            on_confirm=handle_revoke_all
        />
    }
}

// ============================================================================
// MFA Management
// ============================================================================

/// MFA enrollment/management card
#[component]
fn MfaCard() -> impl IntoView {
    let (mfa_status, refetch) =
        use_api_resource(|client: Arc<ApiClient>| async move { client.mfa_status().await });

    view! {
        <Card
            title="Multi-Factor Authentication".to_string()
            description="Add an extra layer of security to your account with TOTP-based MFA.".to_string()
        >
            {move || {
                match mfa_status.try_get().unwrap_or(LoadingState::Loading) {
                    LoadingState::Idle | LoadingState::Loading => {
                        view! {
                            <div class="flex items-center gap-2">
                                <Spinner/>
                                <span class="text-sm text-muted-foreground">"Checking MFA status\u{2026}"</span>
                            </div>
                        }.into_any()
                    }
                    LoadingState::Loaded(status) => {
                        let refetch = refetch;
                        view! {
                            <MfaManager status=status refetch=refetch/>
                        }.into_any()
                    }
                    LoadingState::Error(e) => {
                        // Server errors (5xx) likely mean MFA is not implemented
                        let is_unavailable = matches!(&e, crate::api::ApiError::Server(_));
                        if is_unavailable {
                            view! {
                                <div class="rounded-lg border border-border bg-muted/30 p-4">
                                    <p class="text-sm text-muted-foreground">
                                        "MFA is not available in this deployment."
                                    </p>
                                </div>
                            }.into_any()
                        } else {
                            let refetch = refetch;
                            view! {
                                <ErrorDisplay error=e on_retry=Callback::new(move |_| refetch.run(()))/>
                            }.into_any()
                        }
                    }
                }
            }}
        </Card>
    }
}

/// MFA state manager — routes to enroll or disable flows
#[component]
fn MfaManager(status: MfaStatusResponse, refetch: Refetch) -> impl IntoView {
    // Flow state: None = idle, Some(flow) = active flow
    let enroll_flow = RwSignal::new(Option::<MfaEnrollStartResponse>::None);
    let backup_codes = RwSignal::new(Option::<Vec<String>>::None);
    let show_disable = RwSignal::new(false);

    let is_enrolled = status.mfa_enabled;
    let enrolled_at = status.enrolled_at.clone();

    view! {
        <div class="space-y-4">
            // Status display
            <div class="flex items-center justify-between">
                <div class="flex items-center gap-3">
                    <span class="text-sm font-medium">"Status"</span>
                    {if is_enrolled {
                        view! {
                            <Badge variant=BadgeVariant::Success>"Enabled"</Badge>
                        }.into_any()
                    } else {
                        view! {
                            <Badge variant=BadgeVariant::Secondary>"Disabled"</Badge>
                        }.into_any()
                    }}
                </div>
            </div>

            // Enrolled date
            {enrolled_at.map(|at| view! {
                <div class="grid grid-cols-3 gap-4 items-center">
                    <span class="text-sm text-muted-foreground">"Enrolled"</span>
                    <span class="col-span-2 text-sm">{format_compact_ts(&at)}</span>
                </div>
            })}

            // Action buttons
            {if is_enrolled {
                view! {
                    <Button
                        variant=ButtonVariant::Destructive
                        on_click=Callback::new(move |_| show_disable.set(true))
                    >
                        "Disable MFA"
                    </Button>
                    <MfaDisableFlow
                        open=show_disable
                        refetch=refetch
                    />
                }.into_any()
            } else {
                // Show enroll button or active flow
                view! {
                    {move || {
                        if let Some(codes) = backup_codes.try_get().flatten() {
                            view! {
                                <BackupCodesDisplay codes=codes on_done=Callback::new(move |_| {
                                    backup_codes.set(None);
                                    refetch.run(());
                                })/>
                            }.into_any()
                        } else if let Some(enroll_data) = enroll_flow.try_get().flatten() {
                            view! {
                                <MfaEnrollVerify
                                    enroll=enroll_data
                                    on_success=Callback::new(move |codes: Vec<String>| {
                                        enroll_flow.set(None);
                                        backup_codes.set(Some(codes));
                                    })
                                    on_cancel=Callback::new(move |_| enroll_flow.set(None))
                                />
                            }.into_any()
                        } else {
                            view! {
                                <MfaEnrollStart on_started=Callback::new(move |data| enroll_flow.set(Some(data)))/>
                            }.into_any()
                        }
                    }}
                }.into_any()
            }}
        </div>
    }
}

/// Button to start MFA enrollment
#[component]
fn MfaEnrollStart(on_started: Callback<MfaEnrollStartResponse>) -> impl IntoView {
    let loading = RwSignal::new(false);
    let client = use_api_client();

    let handle_start = {
        let client = client.clone();
        Callback::new(move |_| {
            loading.set(true);
            let client = client.clone();
            wasm_bindgen_futures::spawn_local(async move {
                match client.mfa_start().await {
                    Ok(resp) => {
                        let _ = loading.try_set(false);
                        on_started.run(resp);
                    }
                    Err(e) => {
                        let _ = loading.try_set(false);
                        report_error_with_toast(
                            &e,
                            "Failed to start MFA enrollment",
                            Some("settings"),
                            true,
                        );
                    }
                }
            });
        })
    };

    view! {
        <Button
            variant=ButtonVariant::Primary
            loading=Signal::from(loading)
            on_click=handle_start
        >
            "Enable MFA"
        </Button>
    }
}

/// MFA enrollment verification — shows secret + code input
#[component]
fn MfaEnrollVerify(
    enroll: MfaEnrollStartResponse,
    on_success: Callback<Vec<String>>,
    on_cancel: Callback<()>,
) -> impl IntoView {
    let totp_code = RwSignal::new(String::new());
    let verifying = RwSignal::new(false);
    let error_msg = RwSignal::new(Option::<String>::None);

    let secret = enroll.secret.clone();
    let otpauth_url = enroll.otpauth_url.clone();
    let client = use_api_client();

    let handle_verify = {
        let client = client.clone();
        Callback::new(move |_| {
            let code = totp_code.get_untracked();
            if code.len() != 6 {
                error_msg.set(Some("Enter a 6-digit code.".to_string()));
                return;
            }
            verifying.set(true);
            error_msg.set(None);
            let code = code.clone();
            let client = client.clone();
            wasm_bindgen_futures::spawn_local(async move {
                let req = MfaEnrollVerifyRequest { totp_code: code };
                match client.mfa_verify(&req).await {
                    Ok(resp) => {
                        let _ = verifying.try_set(false);
                        on_success.run(resp.backup_codes);
                    }
                    Err(e) => {
                        let _ = verifying.try_set(false);
                        error_msg.set(Some(e.user_message()));
                    }
                }
            });
        })
    };

    view! {
        <div class="space-y-4 rounded-lg border border-border p-4">
            <h4 class="text-sm font-semibold">"Set up authenticator"</h4>

            <div class="space-y-2">
                <p class="text-xs text-muted-foreground">
                    "Add this account to your authenticator app using the secret below or the otpauth URL."
                </p>

                // Secret key
                <div class="space-y-1">
                    <p class="text-xs font-medium text-muted-foreground">"Secret Key"</p>
                    <div class="rounded-md bg-muted/50 p-2 font-mono text-xs select-all break-all">
                        {secret}
                    </div>
                </div>

                // OTPAuth URL
                <div class="space-y-1">
                    <p class="text-xs font-medium text-muted-foreground">"OTPAuth URL"</p>
                    <div class="rounded-md bg-muted/50 p-2 font-mono text-xs select-all break-all">
                        {otpauth_url}
                    </div>
                </div>
            </div>

            // Verification code input
            <div class="space-y-2">
                <label for="mfa-enroll-verification-code" class="text-sm font-medium">
                    "Verification Code"
                </label>
                <input
                    id="mfa-enroll-verification-code"
                    type="text"
                    inputmode="numeric"
                    maxlength="6"
                    placeholder="000000"
                    class="w-full rounded-md border border-border bg-background px-3 py-2 font-mono text-center text-lg tracking-widest"
                    prop:value=move || totp_code.try_get().unwrap_or_default()
                    on:input=move |ev| {
                        let val = event_target_value(&ev);
                        let digits: String = val.chars().filter(|c| c.is_ascii_digit()).collect();
                        totp_code.set(digits);
                    }
                />
            </div>

            // Error message
            {move || error_msg.try_get().flatten().map(|msg| view! {
                <p class="text-sm text-destructive">{msg}</p>
            })}

            // Actions
            <div class="flex items-center gap-2">
                <Button
                    variant=ButtonVariant::Primary
                    loading=Signal::from(verifying)
                    on_click=handle_verify
                >
                    "Verify"
                </Button>
                <Button
                    variant=ButtonVariant::Ghost
                    on_click=Callback::new(move |_| on_cancel.run(()))
                >
                    "Cancel"
                </Button>
            </div>
        </div>
    }
}

/// Display backup codes after successful MFA enrollment
#[component]
fn BackupCodesDisplay(codes: Vec<String>, on_done: Callback<()>) -> impl IntoView {
    view! {
        <div class="space-y-4 rounded-lg border border-border p-4">
            <h4 class="text-sm font-semibold">"Backup Codes"</h4>
            <p class="text-xs text-muted-foreground">
                "Save these backup codes in a secure location. Each code can only be used once."
            </p>

            <div class="grid grid-cols-2 gap-2">
                {codes.into_iter().map(|code| view! {
                    <div class="rounded-md bg-muted/50 px-3 py-1.5 font-mono text-sm text-center select-all">
                        {code}
                    </div>
                }).collect_view()}
            </div>

            <Button
                variant=ButtonVariant::Primary
                on_click=Callback::new(move |_| on_done.run(()))
            >
                "I have saved my backup codes"
            </Button>
        </div>
    }
}

/// MFA disable flow — confirm dialog with TOTP input
#[component]
fn MfaDisableFlow(open: RwSignal<bool>, refetch: Refetch) -> impl IntoView {
    let totp_code = RwSignal::new(String::new());
    let disabling = RwSignal::new(false);
    let error_msg = RwSignal::new(Option::<String>::None);
    let client = use_api_client();

    let handle_disable = {
        let client = client.clone();
        Callback::new(move |_| {
            let code = totp_code.get_untracked();
            if code.is_empty() {
                error_msg.set(Some("Enter your TOTP code or a backup code.".to_string()));
                return;
            }
            disabling.set(true);
            error_msg.set(None);

            let refetch = refetch;
            let code = code.clone();
            let client = client.clone();
            wasm_bindgen_futures::spawn_local(async move {
                // Determine if code looks like a TOTP (6 digits) or backup code
                let req = if code.len() == 6 && code.chars().all(|c| c.is_ascii_digit()) {
                    MfaDisableRequest {
                        totp_code: Some(code),
                        backup_code: None,
                    }
                } else {
                    MfaDisableRequest {
                        totp_code: None,
                        backup_code: Some(code),
                    }
                };
                match client.mfa_disable(&req).await {
                    Ok(_) => {
                        let _ = disabling.try_set(false);
                        let _ = open.try_set(false);
                        totp_code.set(String::new());
                        refetch.run(());
                    }
                    Err(e) => {
                        let _ = disabling.try_set(false);
                        error_msg.set(Some(e.user_message()));
                    }
                }
            });
        })
    };

    // Reset state when dialog closes
    Effect::new(move || {
        if !open.try_get().unwrap_or(false) {
            totp_code.set(String::new());
            error_msg.set(None);
        }
    });

    view! {
        <Dialog open=open title="Disable MFA">
            <div class="space-y-4">
                <p class="text-sm text-muted-foreground">
                    "Enter your current TOTP code or a backup code to disable multi-factor authentication."
                </p>
                <div class="space-y-2">
                    <label for="mfa-disable-code" class="text-sm font-medium">"Code"</label>
                    <input
                        id="mfa-disable-code"
                        type="text"
                        placeholder="TOTP or backup code"
                        class="w-full rounded-md border border-border bg-background px-3 py-2 font-mono"
                        prop:value=move || totp_code.try_get().unwrap_or_default()
                        on:input=move |ev| {
                            totp_code.set(event_target_value(&ev));
                        }
                    />
                </div>
                {move || error_msg.try_get().flatten().map(|msg| view! {
                    <p class="text-sm text-destructive">{msg}</p>
                })}
                <div class="flex items-center justify-end gap-2">
                    <Button
                        variant=ButtonVariant::Ghost
                        on_click=Callback::new(move |_| open.set(false))
                    >
                        "Cancel"
                    </Button>
                    <Button
                        variant=ButtonVariant::Destructive
                        loading=Signal::from(disabling)
                        on_click=handle_disable
                    >
                        "Disable MFA"
                    </Button>
                </div>
            </div>
        </Dialog>
    }
}

// ============================================================================
// Helpers
// ============================================================================

/// Truncate a user agent string to a reasonable display length
fn truncate_ua(ua: &str) -> String {
    let char_count = ua.chars().count();
    if char_count > 60 {
        let truncated: String = ua.chars().take(60).collect();
        format!("{truncated}\u{2026}")
    } else {
        ua.to_string()
    }
}

/// Format an ISO timestamp into a compact display form: "Jan 15 10:30"
fn format_compact_ts(iso: &str) -> String {
    if let Some(date_part) = iso.get(..10) {
        if let Some(time_part) = iso.get(11..16) {
            let parts: Vec<&str> = date_part.split('-').collect();
            if parts.len() == 3 {
                let month = match parts[1] {
                    "01" => "Jan",
                    "02" => "Feb",
                    "03" => "Mar",
                    "04" => "Apr",
                    "05" => "May",
                    "06" => "Jun",
                    "07" => "Jul",
                    "08" => "Aug",
                    "09" => "Sep",
                    "10" => "Oct",
                    "11" => "Nov",
                    "12" => "Dec",
                    _ => return iso.to_string(),
                };
                let day = parts[2].trim_start_matches('0');
                return format!("{} {} {}", month, day, time_part);
            }
        }
    }
    iso.to_string()
}
