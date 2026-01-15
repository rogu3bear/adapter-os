//! Style Audit Page (PRD-UI-003)
//!
//! Visual component gallery for baseline snapshots and visual regression testing.
//! Renders all components in all variants for both light and dark modes.

use crate::api::client::{
    InferenceTraceDetailResponse, TimingBreakdown, TokenDecision, TraceReceiptSummary,
};
use crate::components::charts::{
    types::{ChartPoint, DataSeries, TimeSeriesData},
    LineChart, Sparkline, SparklineMetric,
};
use crate::components::start_menu::{StartButton, StartMenu};
use crate::components::trace_viewer::TraceDetailStandalone;
use crate::components::{
    Badge, BadgeVariant, Button, ButtonSize, ButtonVariant, Card, ConfirmationDialog,
    ConfirmationSeverity, DangerZone, DangerZoneItem, Dialog, FormField, InfoBanner, Input,
    Spinner, StatusColor, StatusIndicator, Table, TableBody, TableCell, TableHead, TableHeader,
    TableRow, Textarea, Toggle, WarningBanner,
};
use leptos::prelude::*;

/// Style Audit page - component gallery for visual testing
#[component]
pub fn StyleAudit() -> impl IntoView {
    // Theme state for toggling
    let is_dark = RwSignal::new(false);

    // Toggle theme on the document
    Effect::new(move || {
        if let Some(document) = web_sys::window().and_then(|w| w.document()) {
            if let Some(html) = document.document_element() {
                if is_dark.get() {
                    let _ = html.class_list().add_1("dark");
                } else {
                    let _ = html.class_list().remove_1("dark");
                }
            }
        }
    });

    // Dialog states for demos
    let show_dialog = RwSignal::new(false);
    let show_confirm_normal = RwSignal::new(false);
    let show_confirm_warning = RwSignal::new(false);
    let show_confirm_destructive = RwSignal::new(false);

    // Form state for demos
    let input_value = RwSignal::new(String::new());
    let textarea_value = RwSignal::new(String::new());
    let toggle_checked = RwSignal::new(false);
    let show_start_menu = RwSignal::new(false);

    // Chart mock data
    let chart_data = Memo::new(move |_| {
        let mut data = TimeSeriesData::new();
        let mut series1 = DataSeries::new("Requests", "var(--color-primary, #3b82f6)");
        let mut series2 = DataSeries::new("Errors", "var(--color-red-500, #ef4444)");

        let now = 1700000000000u64; // Fixed baseline timestamp
        for i in 0..20 {
            let t = now + i * 60000;
            let v1 = 50.0 + (i as f64 * 0.5).sin() * 20.0 + (i as f64);
            let v2 = 5.0 + (i as f64 * 0.8).cos() * 2.0;
            series1.push(ChartPoint::new(t, v1));
            series2.push(ChartPoint::new(t, v2));
        }

        data.add_series(series1);
        data.add_series(series2);
        data
    });

    let sparkline_values =
        Signal::derive(move || vec![10.0, 15.0, 12.0, 20.0, 25.0, 22.0, 30.0, 35.0, 28.0, 40.0]);

    // Mock trace data
    let mock_trace = InferenceTraceDetailResponse {
        trace_id: "trc_mock_audit_123".to_string(),
        request_id: Some("req_mock_audit_456".to_string()),
        created_at: "2023-10-27T10:00:00Z".to_string(),
        latency_ms: 450,
        adapters_used: vec!["finance_v1".to_string(), "legal_v2".to_string()],
        token_decisions: vec![
            TokenDecision {
                token_index: 0,
                token_id: Some(101),
                adapter_ids: vec!["finance_v1".to_string()],
                gates_q15: vec![30000],
                entropy: 0.1,
                decision_hash: None,
            },
            TokenDecision {
                token_index: 1,
                token_id: Some(205),
                adapter_ids: vec!["finance_v1".to_string(), "legal_v2".to_string()],
                gates_q15: vec![16000, 15000],
                entropy: 0.8,
                decision_hash: None,
            },
        ],
        timing_breakdown: TimingBreakdown {
            total_ms: 450,
            routing_ms: 50,
            inference_ms: 380,
            policy_ms: 20,
            prefill_ms: None,
            decode_ms: None,
        },
        receipt: Some(TraceReceiptSummary {
            receipt_digest: "digest_123abc".to_string(),
            run_head_hash: "hash_xyz789".to_string(),
            output_digest: "out_456def".to_string(),
            logical_prompt_tokens: 15,
            logical_output_tokens: 42,
            stop_reason_code: Some("stop".to_string()),
            verified: true,
        }),
    };

    let expanded_trace_tokens = RwSignal::new(false);

    view! {
        <div class="min-h-screen bg-background text-foreground p-8">
            // Header with theme toggle
            <div class="max-w-6xl mx-auto">
                <div class="flex items-center justify-between mb-8 pb-4 border-b">
                    <div>
                        <h1 class="text-3xl font-bold">"Style Audit"</h1>
                        <p class="text-muted-foreground mt-1">"PRD-UI-003: Visual Component Gallery"</p>
                    </div>
                    <div class="flex items-center gap-4">
                        <span class="text-sm text-muted-foreground">
                            {move || if is_dark.get() { "Dark Mode" } else { "Light Mode" }}
                        </span>
                        <button
                            class="px-4 py-2 rounded-md border border-input bg-background hover:bg-accent text-sm font-medium"
                            on:click=move |_| is_dark.update(|v| *v = !*v)
                        >
                            {move || if is_dark.get() { "Switch to Light" } else { "Switch to Dark" }}
                        </button>
                    </div>
                </div>

                // Component Sections
                <div class="space-y-12">
                    // ===== CHARTS =====
                    <ComponentSection title="Charts">
                        <SubSection title="Line Chart">
                            <div class="h-64 border rounded p-4 bg-card">
                                <LineChart
                                    data=Signal::from(chart_data)
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
                        <SubSection title="Trace Visualization">
                            <div class="border rounded-lg bg-card">
                                <TraceDetailStandalone
                                    trace=mock_trace
                                    expanded_tokens=expanded_trace_tokens.read_only()
                                    set_expanded_tokens=expanded_trace_tokens.write_only()
                                    compact=false
                                />
                            </div>
                        </SubSection>
                    </ComponentSection>

                    // ===== NAVIGATION =====
                    <ComponentSection title="Navigation">
                        <SubSection title="Start Menu">
                            <div class="h-64 border rounded bg-muted/10 relative p-4 flex items-end">
                                <div class="relative">
                                    <StartButton open=show_start_menu/>
                                    <StartMenu open=show_start_menu/>
                                </div>
                                <p class="ml-4 text-sm text-muted-foreground pb-2">
                                    "Click Start to open the menu (renders fixed at bottom-left of viewport)"
                                </p>
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
                            <InfoBanner
                                title="Information".to_string()
                                message="This is an informational message for the user."
                            />
                            <WarningBanner
                                title="Warning".to_string()
                                message="This action may have unintended consequences."
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

/// Section wrapper for component groups
#[component]
fn ComponentSection(title: &'static str, children: Children) -> impl IntoView {
    view! {
        <section class="space-y-6">
            <h2 class="text-2xl font-semibold border-b pb-2">{title}</h2>
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
