# Code Intelligence UI Screens

## Overview

All UI is implemented in Rust using `aos-ui-web` (Yew framework, compiles to WASM). No external JavaScript dependencies. Screens integrate with `aos-cp-client` for API calls.

---

## Screen 1: Repository Setup

### Purpose
Register repositories and initiate scanning.

### Layout

```
┌────────────────────────────────────────────────────────┐
│ Repository Setup                               [Close] │
├────────────────────────────────────────────────────────┤
│                                                         │
│  Repository ID *                                        │
│  ┌─────────────────────────────────────────────────┐  │
│  │ acme/payments                                    │  │
│  └─────────────────────────────────────────────────┘  │
│                                                         │
│  Repository Path *                                      │
│  ┌─────────────────────────────────────────────────┐  │
│  │ /repos/acme/payments                             │  │
│  └─────────────────────────────────────────────────┘  │
│  [Browse...]                                            │
│                                                         │
│  Languages                                              │
│  ┌─────────────────────────────────────────────────┐  │
│  │ ☑ Python    ☑ TypeScript   ☐ Rust              │  │
│  │ ☐ Go        ☐ Java         ☐ C++               │  │
│  └─────────────────────────────────────────────────┘  │
│                                                         │
│  Frameworks Detected:                                   │
│  ┌─────────────────────────────────────────────────┐  │
│  │ • Django 4.2 (settings.py, manage.py)           │  │
│  │ • pytest 7.4 (pytest.ini)                       │  │
│  └─────────────────────────────────────────────────┘  │
│                                                         │
│  [Cancel]                  [Register & Scan Repo] │
└────────────────────────────────────────────────────────┘
```

### State Management

```rust
#[derive(Default)]
pub struct RepoSetupState {
    pub repo_id: String,
    pub path: String,
    pub languages: HashSet<Language>,
    pub detected_frameworks: Vec<Framework>,
    pub scanning: bool,
    pub error: Option<String>,
}

impl Component for RepoSetup {
    type Message = RepoSetupMsg;
    type Properties = ();
    
    fn update(&mut self, ctx: &Context<Self>, msg: Self::Message) -> bool {
        match msg {
            RepoSetupMsg::SetRepoId(id) => {
                self.state.repo_id = id;
                true
            }
            RepoSetupMsg::BrowsePath => {
                // Open file dialog
                self.browse_path();
                true
            }
            RepoSetupMsg::ToggleLanguage(lang) => {
                if self.state.languages.contains(&lang) {
                    self.state.languages.remove(&lang);
                } else {
                    self.state.languages.insert(lang);
                }
                self.detect_frameworks();
                true
            }
            RepoSetupMsg::RegisterAndScan => {
                self.register_repo(ctx);
                true
            }
        }
    }
    
    fn view(&self, ctx: &Context<Self>) -> Html {
        html! {
            <div class="repo-setup">
                <h2>{"Repository Setup"}</h2>
                // ... form fields
                <button onclick={ctx.link().callback(|_| RepoSetupMsg::RegisterAndScan)}>
                    {"Register & Scan Repo"}
                </button>
            </div>
        }
    }
}
```

---

## Screen 2: Adapters View

### Purpose
Visualize active adapters, tiers, and activation heatmaps.

### Layout

```
┌────────────────────────────────────────────────────────────────────┐
│ Code Adapters                                   [Refresh] [Filter] │
├────────────────────────────────────────────────────────────────────┤
│                                                                     │
│  Tier Breakdown:                                                    │
│  ┌─────────────────────────────────────────────────────────────┐  │
│  │ Base (1)      ████████████████████████████████ 1 adapter    │  │
│  │ Code (1)      ████████████████████████████████ 1 adapter    │  │
│  │ Framework (3) ████████████████ 3 adapters                   │  │
│  │ Codebase (1)  ████████████████████████████████ 1 adapter    │  │
│  │ Ephemeral (2) ████████ 2 adapters                           │  │
│  └─────────────────────────────────────────────────────────────┘  │
│                                                                     │
│  Active Adapters:                                                   │
│  ┌─────────────────────────────────────────────────────────────┐  │
│  │ ID                           Tier      Rank  Activation  TTL │  │
│  ├─────────────────────────────────────────────────────────────┤  │
│  │ code_lang_v1                 code      16    0.42        -   │  │
│  │ framework_django_v1          framework 12    0.28        -   │  │
│  │ framework_pytest_v1          framework 8     0.15        -   │  │
│  │ codebase_acme_payments_v7    codebase  24    0.38        -   │  │
│  │ commit_abc123def [⏱ 68h]     ephemeral 4     0.52      68h   │  │
│  │ commit_def456ghi [⏱ 23h]     ephemeral 4     0.18      23h   │  │
│  └─────────────────────────────────────────────────────────────┘  │
│                                                                     │
│  Activation Heatmap (last 100 requests):                            │
│  ┌─────────────────────────────────────────────────────────────┐  │
│  │ code_lang_v1        ████████████████████░░░░░░░░ 42%        │  │
│  │ codebase_...        ███████████████░░░░░░░░░░░░░ 38%        │  │
│  │ framework_django    ██████████░░░░░░░░░░░░░░░░░░ 28%        │  │
│  │ commit_abc123       █████████████░░░░░░░░░░░░░░░ 32%        │  │
│  └─────────────────────────────────────────────────────────────┘  │
│                                                                     │
│  [Evict Selected] [Train New Adapter] [View Details]              │
└────────────────────────────────────────────────────────────────────┘
```

### Component

```rust
pub struct AdaptersView {
    adapters: Vec<AdapterInfo>,
    activation_data: HashMap<String, f32>,
}

impl Component for AdaptersView {
    type Message = AdaptersMsg;
    
    fn view(&self, ctx: &Context<Self>) -> Html {
        html! {
            <div class="adapters-view">
                <h2>{"Code Adapters"}</h2>
                {self.render_tier_breakdown()}
                {self.render_adapter_table()}
                {self.render_activation_heatmap()}
            </div>
        }
    }
    
    fn render_activation_heatmap(&self) -> Html {
        html! {
            <div class="heatmap">
                {for self.activation_data.iter().map(|(id, pct)| {
                    let width = format!("{}%", pct * 100.0);
                    html! {
                        <div class="heatmap-row">
                            <span class="adapter-id">{id}</span>
                            <div class="bar" style={format!("width: {}", width)}></div>
                            <span class="percentage">{format!("{:.0}%", pct * 100.0)}</span>
                        </div>
                    }
                })}
            </div>
        }
    }
}
```

---

## Screen 3: Commit Inspector

### Purpose
View commit details, diff, impacted symbols, and ephemeral adapter status.

### Layout

```
┌────────────────────────────────────────────────────────────────────┐
│ Commit Inspector: abc123def456                          [Refresh]  │
├────────────────────────────────────────────────────────────────────┤
│                                                                     │
│  Repository: acme/payments         Branch: fix/payment-timeout     │
│  Author: John Doe                  Date: 2025-10-05 11:00:00       │
│  Message: Fix payment timeout handling                             │
│                                                                     │
│  ┌─ Changed Files (3) ──────────────────────────────────────────┐ │
│  │ • src/payments/processor.py        (+25, -10)                 │ │
│  │ • tests/test_processor.py          (+15, -2)                  │ │
│  │ • src/payments/models.py           (+7, -0)                   │ │
│  └───────────────────────────────────────────────────────────────┘ │
│                                                                     │
│  ┌─ Impacted Symbols (5) ────────────────────────────────────────┐ │
│  │ • process_payment (modified)                                  │ │
│  │ • Payment.validate (modified)                                 │ │
│  │ • test_process_payment_timeout (added)                        │ │
│  └───────────────────────────────────────────────────────────────┘ │
│                                                                     │
│  ┌─ Test Results ─────────────────────────────────────────────────┐ │
│  │ Passed: 285  Failed: 2  Skipped: 0                            │ │
│  │                                                                 │ │
│  │ Failures:                                                       │ │
│  │   × test_process_payment_timeout                              │ │
│  │     AssertionError: Expected PaymentTimeoutError              │ │
│  │   × test_payment_retry_logic                                  │ │
│  │     AssertionError: Expected retry after timeout              │ │
│  └───────────────────────────────────────────────────────────────┘ │
│                                                                     │
│  ┌─ Ephemeral Adapter ────────────────────────────────────────────┐ │
│  │ Status: Active    Mode: micro_lora    Rank: 4                 │ │
│  │ TTL: 68h remaining   Activations: 47                          │ │
│  │                                                                 │ │
│  │ [Extend TTL] [Evict] [Promote to Codebase]                    │ │
│  └───────────────────────────────────────────────────────────────┘ │
│                                                                     │
│  [View Diff] [Propose Fix] [Run Tests]                            │
└────────────────────────────────────────────────────────────────────┘
```

---

## Screen 4: Routing Inspector

### Purpose
Understand routing decisions for a specific request.

### Layout

```
┌────────────────────────────────────────────────────────────────────┐
│ Routing Inspector                                      [New Query] │
├────────────────────────────────────────────────────────────────────┤
│                                                                     │
│  Prompt:                                                            │
│  ┌─────────────────────────────────────────────────────────────┐  │
│  │ Fix the failing test test_process_payment_timeout           │  │
│  └─────────────────────────────────────────────────────────────┘  │
│                                                                     │
│  Context File: src/payments/processor.py                            │
│                                                                     │
│  ┌─ Extracted Features ───────────────────────────────────────────┐ │
│  │ Language: Python                                               │ │
│  │ Framework Prior: django (2.0), pytest (2.0)                   │ │
│  │ Symbol Hits: 3.0 (process_payment, Payment, Transaction)      │ │
│  │ Path Tokens: [src, payments, processor]                       │ │
│  │ Commit Hint: 1.0 (ephemeral exists)                           │ │
│  │ Prompt Verb: fix                                               │ │
│  │ Attention Entropy: 0.42                                        │ │
│  └────────────────────────────────────────────────────────────────┘ │
│                                                                     │
│  ┌─ Adapter Scores ────────────────────────────────────────────────┐ │
│  │ Adapter                     Score   Selected  Gate (Q15)       │ │
│  ├────────────────────────────────────────────────────────────────┤ │
│  │ commit_abc123def            3.20    ✓         0.52 (17039)    │ │
│  │ codebase_acme_payments_v7   2.50    ✓         0.31 (10158)    │ │
│  │ framework_pytest_v1         2.00    ✓         0.17 (5570)     │ │
│  │ code_lang_v1                1.30    ✗         -               │ │
│  │ framework_django_v1         1.80    ✗         -               │ │
│  └────────────────────────────────────────────────────────────────┘ │
│                                                                     │
│  ┌─ Decision Rationale ────────────────────────────────────────────┐ │
│  │ • Ephemeral adapter prioritized (commit_hint = 1.0)           │ │
│  │ • High symbol hits favor codebase adapter                     │ │
│  │ • Test focus detected, pytest framework selected              │ │
│  │ • K=3 constraint satisfied, entropy floor applied             │ │
│  └────────────────────────────────────────────────────────────────┘ │
│                                                                     │
│  [Simulate Different Context] [Export Trace]                       │
└────────────────────────────────────────────────────────────────────┘
```

---

## Screen 5: Patch Lab

### Purpose
Propose, review, and apply patches with evidence citations.

### Layout

```
┌────────────────────────────────────────────────────────────────────┐
│ Patch Lab                                    [Save Draft] [Close]  │
├────────────────────────────────────────────────────────────────────┤
│                                                                     │
│  ┌─ Request ──────────────────────────────────────────────────────┐ │
│  │ Prompt:                                                         │ │
│  │ ┌───────────────────────────────────────────────────────────┐ │ │
│  │ │ Fix the failing test by adding timeout handling           │ │ │
│  │ └───────────────────────────────────────────────────────────┘ │ │
│  │                                                                 │ │
│  │ Context Files:                                                  │ │
│  │ • src/payments/processor.py                                    │ │
│  │ • tests/test_processor.py                                      │ │
│  │                                                                 │ │
│  │ [Propose Patch]                                                │ │
│  └────────────────────────────────────────────────────────────────┘ │
│                                                                     │
│  ┌─ Generated Patch ──────────────────────────────────────────────┐ │
│  │ File: src/payments/processor.py                                │ │
│  │ ┌───────────────────────────────────────────────────────────┐ │ │
│  │ │ @@ -60,7 +60,10 @@ def process_payment(amount, currency):│ │ │
│  │ │      result = gateway.charge(...)                          │ │ │
│  │ │ +    try:                                                   │ │ │
│  │ │ +        result = gateway.charge(..., timeout=30)          │ │ │
│  │ │ +    except TimeoutError:                                  │ │ │
│  │ │ +        raise PaymentTimeoutError('Timed out')            │ │ │
│  │ │      return result                                          │ │ │
│  │ └───────────────────────────────────────────────────────────┘ │ │
│  └────────────────────────────────────────────────────────────────┘ │
│                                                                     │
│  ┌─ Rationale ─────────────────────────────────────────────────────┐ │
│  │ The test expects a PaymentTimeoutError when processing takes  │ │
│  │ too long. Added try/except block with timeout parameter.      │ │
│  └────────────────────────────────────────────────────────────────┘ │
│                                                                     │
│  ┌─ Evidence Citations ────────────────────────────────────────────┐ │
│  │ [1] Code: process_payment (src/payments/processor.py:58-112)  │ │
│  │ [2] Test Log: test_process_payment_timeout failed             │ │
│  │     Error: Expected PaymentTimeoutError                        │ │
│  │ [3] Framework: Django gateway.charge() timeout parameter      │ │
│  └────────────────────────────────────────────────────────────────┘ │
│                                                                     │
│  ┌─ Dry Run Results ───────────────────────────────────────────────┐ │
│  │ Status: ✓ Success                                              │ │
│  │ Tests: 287 passed, 0 failed                                    │ │
│  │ Linter: 0 errors, 3 warnings (no change)                      │ │
│  │ Policy: All checks passed                                      │ │
│  └────────────────────────────────────────────────────────────────┘ │
│                                                                     │
│  [Run Dry Run] [Apply Patch] [Reject]                             │
└────────────────────────────────────────────────────────────────────┘
```

---

## Screen 6: Policy Editor

### Purpose
Configure code-specific policies.

### Layout

```
┌────────────────────────────────────────────────────────────────────┐
│ Code Policy Editor                           [Reset] [Save] [Back] │
├────────────────────────────────────────────────────────────────────┤
│                                                                     │
│  Evidence Requirements:                                             │
│  ┌─────────────────────────────────────────────────────────────┐  │
│  │ Minimum Evidence Spans: [1]                                  │  │
│  └─────────────────────────────────────────────────────────────┘  │
│                                                                     │
│  Auto-Apply Settings:                                               │
│  ┌─────────────────────────────────────────────────────────────┐  │
│  │ ☑ Allow Auto-Apply                                           │  │
│  │ Require Test Coverage: [0.80] (80%)                          │  │
│  └─────────────────────────────────────────────────────────────┘  │
│                                                                     │
│  Path Permissions:                                                  │
│  ┌─────────────────────────────────────────────────────────────┐  │
│  │ Allowlist:                   Denylist:                       │  │
│  │ • src/**                     • **/.env*                      │  │
│  │ • lib/**                     • **/secrets/**                 │  │
│  │ • tests/**                   • **/*.pem                      │  │
│  │ [Add Pattern]                [Add Pattern]                   │  │
│  └─────────────────────────────────────────────────────────────┘  │
│                                                                     │
│  Secret Patterns (regex):                                           │
│  ┌─────────────────────────────────────────────────────────────┐  │
│  │ • (?i)(api[_-]?key|password)\s*=\s*['"][^'"]{8,}['"]        │  │
│  │ • (?i)(aws[_-]?access[_-]?key)                               │  │
│  │ • -----BEGIN (RSA |EC )?PRIVATE KEY-----                     │  │
│  │ [Add Pattern] [Test Pattern]                                 │  │
│  └─────────────────────────────────────────────────────────────┘  │
│                                                                     │
│  Patch Limits:                                                      │
│  ┌─────────────────────────────────────────────────────────────┐  │
│  │ Max Patch Size (lines): [500]                                │  │
│  └─────────────────────────────────────────────────────────────┘  │
│                                                                     │
│  [Preview Policy JSON] [Validate] [Apply Changes]                 │
└────────────────────────────────────────────────────────────────────┘
```

---

## Screen 7: Metrics Dashboard

### Purpose
Visualize code intelligence metrics over time.

### Layout

```
┌────────────────────────────────────────────────────────────────────┐
│ Code Intelligence Metrics                       [Refresh] [Export] │
├────────────────────────────────────────────────────────────────────┤
│                                                                     │
│  Time Range: [Last 7 Days ▼]          CP: [code_v7 ▼]             │
│                                                                     │
│  ┌─ Acceptance Rate ────────────────────────────────────────────┐  │
│  │ 87.3%     ↗ +2.1%                                             │  │
│  │ ┌───────────────────────────────────────────────────────────┐│  │
│  ││ │ █                                                         ││  │
│  ││ │ █ █                                                       ││  │
│  ││ │ █ █ █                                                     ││  │
│  ││ │ █ █ █ █ █ █ █                                            ││  │
│  ││ └───────────────────────────────────────────────────────────┘│  │
│  │   Mon  Tue  Wed  Thu  Fri  Sat  Sun                          │  │
│  └───────────────────────────────────────────────────────────────┘  │
│                                                                     │
│  ┌─ Quality Metrics ────────────────────────────────────────────┐  │
│  │ Compile Success:  97.2% ✓     Regression Rate:  2.1% ✓       │  │
│  │ Test Pass@1:      83.5% ✓     Follow-up Fixes:  8.7% ✓       │  │
│  │ Evidence Coverage: 78.3% ✓    Secret Violations: 0 ✓         │  │
│  └───────────────────────────────────────────────────────────────┘  │
│                                                                     │
│  ┌─ Performance ─────────────────────────────────────────────────┐  │
│  │ Latency p95:      1.85s ✓     Router Overhead:  6.3% ✓       │  │
│  │ Throughput:       12 req/s    Adapter Activations:           │  │
│  └───────────────────────────────────────────────────────────────┘  │
│                                                                     │
│  [View Detailed Reports] [Compare CPs] [Export CSV]                │
└────────────────────────────────────────────────────────────────────┘
```

---

## State Management & API Integration

All screens use `aos-cp-client`:

```rust
use aos_cp_client::CodeClient;

pub struct ScreenState {
    client: CodeClient,
    loading: bool,
    error: Option<String>,
    data: Option<ScreenData>,
}

impl ScreenState {
    pub async fn fetch_data(&mut self) {
        self.loading = true;
        match self.client.get_repos().await {
            Ok(repos) => {
                self.data = Some(ScreenData::Repos(repos));
                self.loading = false;
            }
            Err(e) => {
                self.error = Some(e.to_string());
                self.loading = false;
            }
        }
    }
}
```

---

## Styling

Consistent with existing `aos-ui-web` styles:
- Dark theme by default
- Monospace fonts for code
- Color-coded status (green=pass, red=fail, yellow=warn)
- Responsive layout (minimum 1280x720)
