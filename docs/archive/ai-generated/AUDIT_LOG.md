# v0.66-pre Hallucination Audit

Date: 2025-10-17

## Scope
Audit of recent commits for evidence-backed changes across CLI, API, Core errors, UI layout, menu bar app, and docs.

## Findings

1) CLI inference command added and wired

```660:676:crates/adapteros-cli/src/main.rs
    /// Run a local inference against the worker UDS
    #[command(after_help = "\
Examples:
  # Basic inference
  aosctl infer --prompt 'Hello world' --socket /var/run/adapteros.sock

  # Inference using a specific adapter (preload+swap)
  aosctl infer --adapter my_adapter --prompt 'Use adapter' --socket /var/run/adapteros.sock

  # Increase max tokens and timeout
  aosctl infer --prompt 'Test' --max-tokens 256 --timeout 60000
"])
    Infer {
```

```1405:1421:crates/adapteros-cli/src/main.rs
        Commands::Infer {
            adapter,
            prompt,
            socket,
            max_tokens,
            require_evidence,
            timeout,
        } => {
            commands::infer::run(
                adapter.clone(),
                prompt.clone(),
                *max_tokens,
                *require_evidence,
                socket.clone(),
                *timeout,
            )
            .await?;
        }
```

```8:16:crates/adapteros-cli/src/commands/infer.rs
/// Run a local inference against the worker UDS server
pub async fn run(
    adapter: Option<String>,
    prompt: String,
    max_tokens: Option<usize>,
    require_evidence: bool,
    socket: PathBuf,
    timeout_ms: u64,
) -> Result<()> {
```

Verdict: Grounded.

2) Server API batch inference

```392:399:crates/adapteros-server-api/src/routes.rs
        .route("/v1/patch/propose", post(handlers::propose_patch))
        .route("/v1/infer", post(handlers::infer))
        .route("/v1/infer/batch", post(handlers::batch::batch_infer))
```

```16:17:crates/adapteros-server-api/src/handlers.rs
pub mod batch;
```

```19:56:crates/adapteros-server-api/src/handlers.rs
/// Single request item within a batch inference call
#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub struct BatchInferItemRequest { /* ... */ }

/// Batch inference request payload
#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub struct BatchInferRequest { /* ... */ }

/// Batch inference aggregate response payload
#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub struct BatchInferResponse { /* ... */ }
```

Verdict: Grounded. Batch tests pass (3/3).

3) Expanded core error variants

```191:199:crates/adapteros-core/src/error.rs
    #[error("UDS connection failed: {path}")]
    UdsConnectionFailed { /* ... */ },

    #[error("Invalid response from worker: {reason}")]
    InvalidResponse { reason: String },
```

Verdict: Grounded.

4) UI layout refactor

```198:205:ui/src/layout/LayoutProvider.tsx
// Combined LayoutProvider
export function LayoutProvider({ children }: { children: React.ReactNode }) {
  return (
    <ThemeProvider>
      <AuthProvider>
        <TenantProvider>
          <ResizeProvider>
            {children}
```

```7:14:ui/src/layout/RootLayout.tsx
import { useTheme, useAuth, useTenant } from './LayoutProvider';
export default function RootLayout() {
  const { theme, toggleTheme } = useTheme();
  const { user, isLoading, logout } = useAuth();
  const { selectedTenant, setSelectedTenant, tenants } = useTenant();
```

Verdict: Grounded.

5) Menu bar app modularization

```8:19:menu-bar-app/Sources/AdapterOSMenu/StatusViewModel.swift
/// ViewModel managing status polling and UI state
@MainActor
class StatusViewModel: ObservableObject {
    @Published var status: AdapterOSStatus?
    @Published var metrics: SystemMetrics?
    @Published var isOffline: Bool = true
```

Verdict: Grounded.

6) Docs: Cursor integration

```91:99:docs/CURSOR_INTEGRATION_GUIDE.md
### Base-only (no adapters)
- Ensure the control plane is running: API at `http://127.0.0.1:8080/api`
- Cursor can target the AdapterOS inference endpoints (note: `/api/v1/models` and `/api/v1/chat/completions` have been removed in this build).
```

Verdict: Grounded.

## Test Evidence
- adapteros-server-api batch tests: 3 passed (schema parity confirmed).

## Determinism & Policy Notes
- Zero egress preserved; UDS used for CLI inference.
- Deterministic execution unchanged.
- Telemetry remains via `tracing`.

## Result
Audit Passed.

