//! Style Audit Page (PRD-UI-003)
//!
//! Visual component gallery for baseline snapshots and visual regression testing.
//! Renders all components in all variants for both light and dark modes.

use crate::api::{ApiClient, ProcessHealthMetricResponse};
use crate::components::charts::{
    types::{ChartPoint, DataSeries, TimeSeriesData},
    LineChart, Sparkline, SparklineMetric,
};
use crate::components::trace_viewer::TraceDetailStandalone;
use crate::components::{
    AlertBanner, Badge, BadgeVariant, BannerVariant, Button, ButtonSize, ButtonVariant, Card,
    ConfirmationDialog, ConfirmationSeverity, DangerZone, DangerZoneItem, Dialog, ErrorDisplay,
    FormField, Input, Spinner, StatusColor, StatusIndicator, Table, TableBody, TableCell,
    TableHead, TableHeader, TableRow, Textarea, Toggle,
};
use crate::constants::pagination::TOKEN_DECISIONS_PAGE_SIZE;
use crate::hooks::{use_api_resource, LoadingState};
use crate::signals::use_auth;
use chrono::DateTime;
use leptos::prelude::*;
use std::collections::HashMap;
use std::sync::Arc;

/// Style Audit page - component gallery for visual testing
#[component]
pub fn StyleAudit() -> impl IntoView {
    let (auth_state, _) = use_auth();
    let is_authenticated = Signal::derive(move || {
        auth_state
            .try_get()
            .is_some_and(|state| state.is_authenticated())
    });

    // Theme state for toggling
    let is_dark = RwSignal::new(false);

    // Toggle theme on the document
    Effect::new(move || {
        let Some(dark) = is_dark.try_get() else {
            return;
        };
        if let Some(document) = web_sys::window().and_then(|w| w.document()) {
            if let Some(html) = document.document_element() {
                if dark {
                    let _ = html.class_list().add_1("dark");
                } else {
                    let _ = html.class_list().remove_1("dark");
                }
            }
        }
    });

    // Dialog states for examples
    let show_dialog = RwSignal::new(false);
    let show_confirm_normal = RwSignal::new(false);
    let show_confirm_warning = RwSignal::new(false);
    let show_confirm_destructive = RwSignal::new(false);

    // Form state for examples
    let input_value = RwSignal::new(String::new());
    let textarea_value = RwSignal::new(String::new());
    let toggle_checked = RwSignal::new(false);

    // Fetch health metrics for live charts
    let (health_metrics, refetch_health) = use_api_resource({
        move |client: Arc<ApiClient>| {
            let authed = is_authenticated.try_get().unwrap_or(false);
            async move {
                if authed {
                    client.get_process_health_metrics(None).await
                } else {
                    Ok(Vec::new())
                }
            }
        }
    });

    let chart_payload =
        Memo::new(
            move |_| match health_metrics.try_get().unwrap_or(LoadingState::Idle) {
                LoadingState::Loaded(metrics) => build_metric_series(&metrics),
                _ => (TimeSeriesData::new(), Vec::new()),
            },
        );

    let chart_data = Signal::derive(move || {
        chart_payload
            .try_get()
            .map(|p| p.0.clone())
            .unwrap_or_default()
    });
    let sparkline_values = Signal::derive(move || {
        chart_payload
            .try_get()
            .map(|p| p.1.clone())
            .unwrap_or_default()
    });

    // Trace fetch state (live diagnostic)
    let trace_id_input = RwSignal::new(String::new());
    let requested_trace_id = RwSignal::new(String::new());

    // Fetch recent traces on mount, auto-load the first one
    let (recent_traces, refetch_traces) = use_api_resource({
        move |client: Arc<ApiClient>| {
            let authed = is_authenticated.try_get().unwrap_or(false);
            async move {
                if authed {
                    client.list_inference_traces(None, Some(5)).await
                } else {
                    Ok(Vec::new())
                }
            }
        }
    });

    let (trace_detail, refetch_trace) = use_api_resource({
        move |client: Arc<ApiClient>| {
            let trace_id = requested_trace_id.get_untracked();
            let authed = is_authenticated.try_get().unwrap_or(false);
            async move {
                let trace_id = trace_id.trim().to_string();
                if trace_id.is_empty() || !authed {
                    Ok(None)
                } else {
                    client
                        .get_inference_trace_detail(
                            &trace_id,
                            Some(TOKEN_DECISIONS_PAGE_SIZE),
                            None,
                        )
                        .await
                        .map(Some)
                }
            }
        }
    });
    let live_data_bootstrapped = RwSignal::new(false);

    // When auth becomes available after mount, refetch live-backed sections once.
    Effect::new(move || {
        let authed = is_authenticated.try_get().unwrap_or(false);
        if authed {
            if !live_data_bootstrapped.try_get().unwrap_or(false) {
                live_data_bootstrapped.set(true);
                refetch_health.run(());
                refetch_traces.run(());
                if !requested_trace_id
                    .try_get()
                    .unwrap_or_default()
                    .trim()
                    .is_empty()
                {
                    refetch_trace.run(());
                }
            }
        } else {
            live_data_bootstrapped.set(false);
        }
    });

    // Auto-populate with most recent trace when loaded
    Effect::new(move || {
        let Some(state) = recent_traces.try_get() else {
            return;
        };
        if let LoadingState::Loaded(traces) = state {
            if let Some(first) = traces.first() {
                let tid = first.trace_id.clone();
                if requested_trace_id.get_untracked().is_empty() {
                    let _ = trace_id_input.try_set(tid.clone());
                    let _ = requested_trace_id.try_set(tid);
                    refetch_trace.run(());
                }
            }
        }
    });

    let expanded_trace_tokens = RwSignal::new(false);

    view! {
        <div class="min-h-screen bg-background text-foreground p-8">
            // Header with theme toggle
            <div class="max-w-6xl mx-auto">
                <div class="flex items-center justify-between mb-8 pb-4 border-b">
                    <div>
                        <h1 class="heading-1" data-testid="style-audit-heading">"Style Audit"</h1>
                        <p class="text-muted-foreground mt-1">
                            "PRD-UI-003: Visual Component Gallery. Live metrics and trace examples require login."
                        </p>
                    </div>
                    <div class="flex items-center gap-4">
                        <span class="text-sm text-muted-foreground">
                            {move || if is_dark.try_get().unwrap_or(false) { "Dark Mode" } else { "Light Mode" }}
                        </span>
                        <button
                            class="px-4 py-2 rounded-md border border-input bg-background hover:bg-accent text-sm font-medium"
                            on:click=move |_| is_dark.update(|v| *v = !*v)
                        >
                            {move || if is_dark.try_get().unwrap_or(false) { "Switch to Light" } else { "Switch to Dark" }}
                        </button>
                    </div>
                </div>

                {move || {
                    if is_authenticated.try_get().unwrap_or(false) {
                        None
                    } else {
                        Some(view! {
                            <div class="mb-8">
                                <AlertBanner
                                    title="Public page".to_string()
                                    message="Static component previews work without sign-in. Live API-backed examples require an authenticated session."
                                    variant=BannerVariant::Info
                                />
                            </div>
                        })
                    }
                }}

                // Component Sections
                <div class="space-y-12">
                    // ===== CHARTS =====
                    <ComponentSection title="Charts">
                        <SubSection title="Line Chart">
                            <div class="h-64 border rounded p-4 bg-card">
                                <LineChart
                                    data=chart_data
                                    title="Traffic Overview".to_string()
                                    y_label="Requests/sec".to_string()
                                    height=200.0
                                    show_points=true
                                />
                            </div>
                        </SubSection>

                        <SubSection title="Sparklines">
                            <div class="grid gap-4 md:grid-cols-3">
                                <div class="p-4 border rounded bg-card">
                                    <h4 class="text-sm text-muted-foreground mb-2">"Simple Sparkline"</h4>
                                    <Sparkline values=sparkline_values width=120 height=30 fill=true />
                                </div>

                                <div class="p-4 border rounded bg-card">
                                    <h4 class="text-sm text-muted-foreground mb-2">"Trend Color"</h4>
                                    <Sparkline values=sparkline_values width=120 height=30 fill=true trend_color=true />
                                </div>

                                <SparklineMetric
                                    label="CPU Usage".to_string()
                                    value="42%".to_string()
                                    values=sparkline_values
                                    class="border rounded p-4 bg-card".to_string()
                                />
                            </div>
                        </SubSection>
                    </ComponentSection>

                    // ===== INFERENCE =====
                    <ComponentSection title="Inference">
                        <SubSection title="Trace Visualization (requires login)">
                            <div class="border rounded-lg bg-card p-4 space-y-4">
                                <div class="flex flex-wrap items-end gap-3">
                                    <div class="w-full sm:w-80">
                                        <Input
                                            value=trace_id_input
                                            label="Trace ID".to_string()
                                            placeholder="trc_...".to_string()
                                        />
                                    </div>
                                    <Button
                                        variant=ButtonVariant::Primary
                                        on_click=Callback::new(move |_| {
                                            let trace_id = trace_id_input.try_get().unwrap_or_default().trim().to_string();
                                            requested_trace_id.set(trace_id.clone());
                                            if !trace_id.is_empty() {
                                                refetch_trace.run(());
                                            }
                                        })
                                    >
                                        "Load Trace"
                                    </Button>
                                    {move || {
                                        let current = requested_trace_id.try_get().unwrap_or_default();
                                        if current.trim().is_empty() {
                                            None
                                        } else {
                                            Some(view! {
                                                <span class="text-xs text-muted-foreground">
                                                    "Loaded: " <span class="font-mono">{current}</span>
                                                </span>
                                            })
                                        }
                                    }}
                                </div>
                                <div>
                                    {move || {
                                        // Show loading state while fetching recent traces
                                        if matches!(recent_traces.try_get().unwrap_or(LoadingState::Idle), LoadingState::Idle | LoadingState::Loading)
                                            && requested_trace_id.try_get().unwrap_or_default().is_empty()
                                        {
                                            return view! {
                                                <div class="flex items-center gap-2 text-muted-foreground">
                                                    <Spinner/>
                                                    <span>"Loading recent traces..."</span>
                                                </div>
                                            }
                                            .into_any();
                                        }

                                        // No traces available
                                        if let LoadingState::Loaded(traces) = recent_traces.try_get().unwrap_or(LoadingState::Idle) {
                                            if traces.is_empty() && requested_trace_id.try_get().unwrap_or_default().is_empty() {
                                                if !is_authenticated.try_get().unwrap_or(false) {
                                                    return view! {
                                                        <AlertBanner
                                                            title="Sign in required".to_string()
                                                            message="Live trace samples are only available in authenticated sessions."
                                                            variant=BannerVariant::Info
                                                        />
                                                    }
                                                    .into_any();
                                                }
                                                return view! {
                                                    <div class="text-sm text-muted-foreground">
                                                        "No inference traces available. Run an inference to generate traces."
                                                    </div>
                                                }
                                                .into_any();
                                            }
                                        }

                                        // Error fetching recent traces
                                        if let LoadingState::Error(err) = recent_traces.try_get().unwrap_or(LoadingState::Idle) {
                                            if requested_trace_id.try_get().unwrap_or_default().is_empty() {
                                                if err.requires_auth() {
                                                    return view! {
                                                        <AlertBanner
                                                            title="Sign in required".to_string()
                                                            message="Live trace samples are only available in authenticated sessions."
                                                            variant=BannerVariant::Info
                                                        />
                                                    }
                                                    .into_any();
                                                }
                                                return view! {
                                                    <ErrorDisplay error=err.clone()/>
                                                }
                                                .into_any();
                                            }
                                        }

                                        // Show trace detail or loading state
                                        if requested_trace_id.try_get().unwrap_or_default().trim().is_empty() {
                                            view! {
                                                <div class="text-sm text-muted-foreground">
                                                    "Enter a trace ID to load trace data. Sign in is required for live traces."
                                                </div>
                                            }
                                            .into_any()
                                        } else {
                                            match trace_detail.try_get().unwrap_or(LoadingState::Idle) {
                                                LoadingState::Idle | LoadingState::Loading => view! {
                                                    <div class="flex items-center gap-2 text-muted-foreground">
                                                        <Spinner/>
                                                        <span>"Loading trace data..."</span>
                                                    </div>
                                                }
                                                .into_any(),
                                                LoadingState::Loaded(Some(detail)) => view! {
                                                    <TraceDetailStandalone
                                                        trace=detail
                                                        expanded_tokens=expanded_trace_tokens.read_only()
                                                        set_expanded_tokens=expanded_trace_tokens.write_only()
                                                        compact=false
                                                    />
                                                }
                                                .into_any(),
                                                LoadingState::Loaded(None) => view! {
                                                    <AlertBanner
                                                        title="No trace data".to_string()
                                                        message="No trace returned for that ID."
                                                            .to_string()
                                                        variant=BannerVariant::Warning
                                                    />
                                                }
                                                .into_any(),
                                                LoadingState::Error(err) => view! {
                                                    {if err.requires_auth() {
                                                        view! {
                                                            <AlertBanner
                                                                title="Sign in required".to_string()
                                                                message="Live trace detail is only available in authenticated sessions."
                                                                variant=BannerVariant::Info
                                                            />
                                                        }.into_any()
                                                    } else {
                                                        view! {
                                                            <ErrorDisplay error=err.clone()/>
                                                        }.into_any()
                                                    }}
                                                }
                                                .into_any(),
                                            }
                                        }
                                    }}
                                </div>
                            </div>
                        </SubSection>
                    </ComponentSection>

                    // ===== BUTTONS =====
                    <ComponentSection title="Buttons">
                        <SubSection title="Variants">
                            <div class="flex flex-wrap gap-4">
                                <Button variant=ButtonVariant::Primary>"Primary"</Button>
                                <Button variant=ButtonVariant::Secondary>"Secondary"</Button>
                                <Button variant=ButtonVariant::Outline>"Outline"</Button>
                                <Button variant=ButtonVariant::Ghost>"Ghost"</Button>
                                <Button variant=ButtonVariant::Destructive>"Destructive"</Button>
                            </div>
                        </SubSection>

                        <SubSection title="Sizes">
                            <div class="flex flex-wrap items-center gap-4">
                                <Button size=ButtonSize::Sm>"Small"</Button>
                                <Button size=ButtonSize::Md>"Medium"</Button>
                                <Button size=ButtonSize::Lg>"Large"</Button>
                                <Button size=ButtonSize::Icon>
                                    <svg xmlns="http://www.w3.org/2000/svg" class="h-4 w-4" fill="none" viewBox="0 0 24 24" stroke="currentColor">
                                        <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M12 4v16m8-8H4"/>
                                    </svg>
                                </Button>
                            </div>
                        </SubSection>

                        <SubSection title="States">
                            <div class="flex flex-wrap gap-4">
                                <Button>"Normal"</Button>
                                <Button disabled=true>"Disabled"</Button>
                                <Button loading=true>"Loading"</Button>
                            </div>
                        </SubSection>
                    </ComponentSection>

                    // ===== BADGES =====
                    <ComponentSection title="Badges">
                        <div class="flex flex-wrap gap-4">
                            <Badge variant=BadgeVariant::Default>"Default"</Badge>
                            <Badge variant=BadgeVariant::Secondary>"Secondary"</Badge>
                            <Badge variant=BadgeVariant::Success>"Success"</Badge>
                            <Badge variant=BadgeVariant::Warning>"Warning"</Badge>
                            <Badge variant=BadgeVariant::Destructive>"Destructive"</Badge>
                            <Badge variant=BadgeVariant::Outline>"Outline"</Badge>
                        </div>
                    </ComponentSection>

                    // ===== STATUS INDICATORS =====
                    <ComponentSection title="Status Indicators">
                        <SubSection title="Colors">
                            <div class="flex flex-wrap gap-6">
                                <StatusIndicator color=StatusColor::Gray label="Gray".to_string()/>
                                <StatusIndicator color=StatusColor::Green label="Green".to_string()/>
                                <StatusIndicator color=StatusColor::Yellow label="Yellow".to_string()/>
                                <StatusIndicator color=StatusColor::Red label="Red".to_string()/>
                                <StatusIndicator color=StatusColor::Blue label="Blue".to_string()/>
                            </div>
                        </SubSection>

                        <SubSection title="Pulsing">
                            <div class="flex flex-wrap gap-6">
                                <StatusIndicator color=StatusColor::Green pulsing=true label="Active".to_string()/>
                                <StatusIndicator color=StatusColor::Yellow pulsing=true label="Warning".to_string()/>
                                <StatusIndicator color=StatusColor::Red pulsing=true label="Error".to_string()/>
                            </div>
                        </SubSection>
                    </ComponentSection>

                    // ===== SPINNER =====
                    <ComponentSection title="Spinner">
                        <div class="flex items-center gap-8">
                            <Spinner/>
                            <span class="text-muted-foreground">"Loading state indicator"</span>
                        </div>
                    </ComponentSection>

                    // ===== INPUTS =====
                    <ComponentSection title="Form Inputs">
                        <div class="grid gap-6 max-w-md">
                            <Input
                                value=input_value
                                label="Text Input".to_string()
                                placeholder="Enter text...".to_string()
                            />
                            <Input
                                value=RwSignal::new(String::new())
                                label="Disabled Input".to_string()
                                placeholder="Cannot edit".to_string()
                                disabled=true
                            />
                            <Textarea
                                value=textarea_value
                                label="Textarea".to_string()
                                placeholder="Enter longer text...".to_string()
                            />
                        </div>
                    </ComponentSection>

                    // ===== FORM FIELDS =====
                    <ComponentSection title="Form Fields">
                        <div class="grid gap-6 max-w-md">
                            <FormField
                                label="Required Field"
                                name="required"
                                required=true
                                help="This field is required"
                            >
                                <Input
                                    value=RwSignal::new(String::new())
                                    placeholder="Required value".to_string()
                                />
                            </FormField>
                            <FormField
                                label="Field with Error"
                                name="error"
                                error=Signal::derive(|| Some("This field has an error".to_string()))
                            >
                                <Input
                                    value=RwSignal::new(String::new())
                                    placeholder="Invalid value".to_string()
                                />
                            </FormField>
                        </div>
                    </ComponentSection>

                    // ===== TOGGLE =====
                    <ComponentSection title="Toggle">
                        <div class="max-w-md">
                            <Toggle
                                checked=toggle_checked
                                label="Enable Feature".to_string()
                                description="Toggle this setting on or off".to_string()
                            />
                        </div>
                    </ComponentSection>

                    // ===== CARDS =====
                    <ComponentSection title="Cards">
                        <div class="grid gap-4 md:grid-cols-2 lg:grid-cols-3">
                            <Card>
                                <p class="text-sm text-muted-foreground">"Simple card content"</p>
                            </Card>
                            <Card title="Card with Title".to_string()>
                                <p class="text-sm text-muted-foreground">"Card with a title prop"</p>
                            </Card>
                            <Card title="Card with Description".to_string() description="A helpful description".to_string()>
                                <p class="text-sm text-muted-foreground">"Card with title and description"</p>
                            </Card>
                        </div>
                    </ComponentSection>

                    // ===== TABLE =====
                    <ComponentSection title="Table">
                        <Card>
                            <Table>
                                <TableHeader>
                                    <TableRow>
                                        <TableHead>"Name"</TableHead>
                                        <TableHead>"Status"</TableHead>
                                        <TableHead>"Actions"</TableHead>
                                    </TableRow>
                                </TableHeader>
                                <TableBody>
                                    <TableRow>
                                        <TableCell>"Item One"</TableCell>
                                        <TableCell><Badge variant=BadgeVariant::Success>"Active"</Badge></TableCell>
                                        <TableCell><Button size=ButtonSize::Sm variant=ButtonVariant::Outline>"View"</Button></TableCell>
                                    </TableRow>
                                    <TableRow>
                                        <TableCell>"Item Two"</TableCell>
                                        <TableCell><Badge variant=BadgeVariant::Warning>"Pending"</Badge></TableCell>
                                        <TableCell><Button size=ButtonSize::Sm variant=ButtonVariant::Outline>"View"</Button></TableCell>
                                    </TableRow>
                                    <TableRow>
                                        <TableCell>"Item Three"</TableCell>
                                        <TableCell><Badge variant=BadgeVariant::Destructive>"Error"</Badge></TableCell>
                                        <TableCell><Button size=ButtonSize::Sm variant=ButtonVariant::Outline>"View"</Button></TableCell>
                                    </TableRow>
                                </TableBody>
                            </Table>
                        </Card>
                    </ComponentSection>

                    // ===== BANNERS =====
                    <ComponentSection title="Banners">
                        <div class="space-y-4 max-w-2xl">
                            <AlertBanner
                                title="Information".to_string()
                                message="This is an informational message for the user."
                                variant=BannerVariant::Info
                            />
                            <AlertBanner
                                title="Warning".to_string()
                                message="This action may have unintended consequences."
                                variant=BannerVariant::Warning
                            />
                            <AlertBanner
                                title="Success".to_string()
                                message="Configuration was saved and propagated to workers."
                                variant=BannerVariant::Success
                            />
                            <AlertBanner
                                title="Error".to_string()
                                message="Unable to complete the operation. Retry or inspect logs."
                                variant=BannerVariant::Error
                            />
                        </div>
                    </ComponentSection>

                    // ===== DIALOGS =====
                    <ComponentSection title="Dialogs">
                        <div class="flex flex-wrap gap-4">
                            <Button on_click=Callback::new(move |_| show_dialog.set(true))>
                                "Open Dialog"
                            </Button>
                        </div>

                        <Dialog
                            open=show_dialog
                            title="Example Dialog".to_string()
                            description="This is an example dialog with a title and description.".to_string()
                        >
                            <p class="text-sm text-muted-foreground">"Dialog content goes here."</p>
                            <div class="flex justify-end gap-2 mt-4">
                                <Button variant=ButtonVariant::Outline on_click=Callback::new(move |_| show_dialog.set(false))>
                                    "Cancel"
                                </Button>
                                <Button on_click=Callback::new(move |_| show_dialog.set(false))>
                                    "Confirm"
                                </Button>
                            </div>
                        </Dialog>
                    </ComponentSection>

                    // ===== CONFIRMATION DIALOGS =====
                    <ComponentSection title="Confirmation Dialogs">
                        <div class="flex flex-wrap gap-4">
                            <Button variant=ButtonVariant::Outline on_click=Callback::new(move |_| show_confirm_normal.set(true))>
                                "Normal Confirm"
                            </Button>
                            <Button variant=ButtonVariant::Outline on_click=Callback::new(move |_| show_confirm_warning.set(true))>
                                "Warning Confirm"
                            </Button>
                            <Button variant=ButtonVariant::Destructive on_click=Callback::new(move |_| show_confirm_destructive.set(true))>
                                "Destructive Confirm"
                            </Button>
                        </div>

                        <ConfirmationDialog
                            open=show_confirm_normal
                            title="Confirm Action"
                            description="Are you sure you want to proceed with this action?"
                            severity=ConfirmationSeverity::Normal
                            on_confirm=Callback::new(move |_| show_confirm_normal.set(false))
                        />

                        <ConfirmationDialog
                            open=show_confirm_warning
                            title="Warning"
                            description="This action may have side effects. Please confirm you want to continue."
                            severity=ConfirmationSeverity::Warning
                            on_confirm=Callback::new(move |_| show_confirm_warning.set(false))
                        />

                        <ConfirmationDialog
                            open=show_confirm_destructive
                            title="Delete Item"
                            description="This will permanently delete the item. This action cannot be undone."
                            severity=ConfirmationSeverity::Destructive
                            typed_confirmation="DELETE".to_string()
                            confirm_text="Delete".to_string()
                            on_confirm=Callback::new(move |_| show_confirm_destructive.set(false))
                        />
                    </ComponentSection>

                    // ===== DANGER ZONE =====
                    <ComponentSection title="Danger Zone">
                        <DangerZone>
                            <DangerZoneItem
                                title="Delete Account"
                                description="Permanently delete your account and all associated data."
                            >
                                <Button variant=ButtonVariant::Destructive size=ButtonSize::Sm>
                                    "Delete Account"
                                </Button>
                            </DangerZoneItem>
                            <DangerZoneItem
                                title="Reset Settings"
                                description="Reset all settings to their default values."
                            >
                                <Button variant=ButtonVariant::Destructive size=ButtonSize::Sm>
                                    "Reset"
                                </Button>
                            </DangerZoneItem>
                        </DangerZone>
                    </ComponentSection>

                    // ===== COLOR PALETTE =====
                    <ComponentSection title="Color Palette">
                        <SubSection title="Semantic Colors">
                            <div class="grid grid-cols-2 md:grid-cols-4 gap-4">
                                <ColorSwatch color="bg-background" label="Background"/>
                                <ColorSwatch color="bg-foreground" label="Foreground"/>
                                <ColorSwatch color="bg-muted" label="Muted"/>
                                <ColorSwatch color="bg-muted-foreground" label="Muted FG"/>
                                <ColorSwatch color="bg-primary" label="Primary"/>
                                <ColorSwatch color="bg-secondary" label="Secondary"/>
                                <ColorSwatch color="bg-accent" label="Accent"/>
                                <ColorSwatch color="bg-destructive" label="Destructive"/>
                            </div>
                        </SubSection>

                        <SubSection title="Status Colors">
                            <div class="grid grid-cols-2 md:grid-cols-5 gap-4">
                                <ColorSwatch color="bg-green-500" label="Success"/>
                                <ColorSwatch color="bg-yellow-500" label="Warning"/>
                                <ColorSwatch color="bg-red-500" label="Error"/>
                                <ColorSwatch color="bg-blue-500" label="Info"/>
                                <ColorSwatch color="bg-gray-500" label="Neutral"/>
                            </div>
                        </SubSection>
                    </ComponentSection>

                    // ===== TYPOGRAPHY =====
                    <ComponentSection title="Typography">
                        <div class="space-y-4">
                            <div><h1 class="text-4xl font-bold">"Heading 1 (text-4xl)"</h1></div>
                            <div><h2 class="text-3xl font-bold">"Heading 2 (text-3xl)"</h2></div>
                            <div><h3 class="text-2xl font-semibold">"Heading 3 (text-2xl)"</h3></div>
                            <div><h4 class="text-xl font-semibold">"Heading 4 (text-xl)"</h4></div>
                            <div><h5 class="text-lg font-medium">"Heading 5 (text-lg)"</h5></div>
                            <div><p class="text-base">"Body text (text-base)"</p></div>
                            <div><p class="text-sm text-muted-foreground">"Small/muted text (text-sm)"</p></div>
                            <div><p class="text-xs text-muted-foreground">"Extra small (text-xs)"</p></div>
                            <div><code class="font-mono text-sm bg-muted px-1.5 py-0.5 rounded">"Monospace code"</code></div>
                        </div>
                    </ComponentSection>

                    // ===== SPACING REFERENCE =====
                    <ComponentSection title="Spacing Reference">
                        <div class="flex flex-wrap gap-4 items-end">
                            <SpacingBox size="1" label="0.25rem"/>
                            <SpacingBox size="2" label="0.5rem"/>
                            <SpacingBox size="3" label="0.75rem"/>
                            <SpacingBox size="4" label="1rem"/>
                            <SpacingBox size="6" label="1.5rem"/>
                            <SpacingBox size="8" label="2rem"/>
                            <SpacingBox size="12" label="3rem"/>
                        </div>
                    </ComponentSection>
                </div>

                // Footer
                <div class="mt-12 pt-8 border-t text-center text-sm text-muted-foreground">
                    <p>"adapterOS Style Audit • PRD-UI-003"</p>
                    <p class="mt-1">"Use browser screenshot tools to capture baseline visuals"</p>
                </div>
            </div>
        </div>
    }
}

fn build_metric_series(metrics: &[ProcessHealthMetricResponse]) -> (TimeSeriesData, Vec<f64>) {
    let mut data = TimeSeriesData::new();
    if metrics.is_empty() {
        return (data, Vec::new());
    }

    let mut grouped: HashMap<String, Vec<ChartPoint>> = HashMap::new();
    for metric in metrics {
        let ts = DateTime::parse_from_rfc3339(&metric.collected_at)
            .ok()
            .and_then(|dt| dt.timestamp_millis().try_into().ok());
        if let Some(timestamp) = ts {
            grouped
                .entry(metric.metric_name.clone())
                .or_default()
                .push(ChartPoint::new(timestamp, metric.metric_value));
        }
    }

    if grouped.is_empty() {
        return (data, Vec::new());
    }

    let mut names: Vec<String> = grouped.keys().cloned().collect();
    names.sort();
    let primary_name = if names.iter().any(|n| n == "cpu_usage_percent") {
        "cpu_usage_percent".to_string()
    } else {
        names[0].clone()
    };

    let mut primary_points = grouped.remove(&primary_name).unwrap_or_default();
    primary_points.sort_by_key(|p| p.timestamp);
    if primary_points.len() > 20 {
        primary_points.drain(0..primary_points.len().saturating_sub(20));
    }
    let mut primary_series = DataSeries::new(primary_name.clone(), "var(--color-primary, #3b82f6)");
    for point in &primary_points {
        primary_series.push(point.clone());
    }
    data.add_series(primary_series);

    if let Some(secondary_name) = names.into_iter().find(|n| n != &primary_name) {
        let mut secondary_points = grouped.remove(&secondary_name).unwrap_or_default();
        secondary_points.sort_by_key(|p| p.timestamp);
        if secondary_points.len() > 20 {
            secondary_points.drain(0..secondary_points.len().saturating_sub(20));
        }
        let mut secondary_series = DataSeries::new(secondary_name, "var(--color-red-500, #ef4444)");
        for point in &secondary_points {
            secondary_series.push(point.clone());
        }
        data.add_series(secondary_series);
    }

    let sparkline_values = primary_points
        .iter()
        .rev()
        .take(10)
        .rev()
        .map(|p| p.value)
        .collect();

    (data, sparkline_values)
}

/// Section wrapper for component groups
#[component]
fn ComponentSection(title: &'static str, children: Children) -> impl IntoView {
    view! {
        <section class="space-y-6">
            <h2 class="heading-2 border-b pb-2">{title}</h2>
            <div class="space-y-6">
                {children()}
            </div>
        </section>
    }
}

/// Subsection for variant groups
#[component]
fn SubSection(title: &'static str, children: Children) -> impl IntoView {
    view! {
        <div class="space-y-3">
            <h3 class="text-sm font-medium text-muted-foreground uppercase tracking-wider">{title}</h3>
            {children()}
        </div>
    }
}

/// Color swatch display
#[component]
fn ColorSwatch(color: &'static str, label: &'static str) -> impl IntoView {
    view! {
        <div class="space-y-2">
            <div class=format!("h-12 rounded-md border {}", color)></div>
            <p class="text-xs text-center text-muted-foreground">{label}</p>
        </div>
    }
}

/// Spacing reference box
#[component]
fn SpacingBox(size: &'static str, label: &'static str) -> impl IntoView {
    let size_class = match size {
        "1" => "w-1 h-1",
        "2" => "w-2 h-2",
        "3" => "w-3 h-3",
        "4" => "w-4 h-4",
        "6" => "w-6 h-6",
        "8" => "w-8 h-8",
        "12" => "w-12 h-12",
        _ => "w-4 h-4",
    };

    view! {
        <div class="flex flex-col items-center gap-1">
            <div class=format!("{} bg-primary rounded", size_class)></div>
            <span class="text-xs text-muted-foreground">{label}</span>
        </div>
    }
}
