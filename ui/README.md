# AdapterOS Control Plane UI

Modern React-based web interface for managing the AdapterOS control plane.


## M0 Features

The UI provides core management functionality for the AdapterOS control plane:

- **Authentication**: JWT-based login with httpOnly cookies
- **Dashboard**: System overview and key metrics
- **Management**:
  - Tenants: Multi-tenant configuration
  - Adapters: LoRA adapter management
  - Policies: Security policy configuration
  - Metrics: System performance monitoring
- **Operations**:
  - Inference: Run inference with adapters
  - Telemetry: Event logs and monitoring
  - Audit: Security audit trails

## Routing (React Router v6)

Core M0 Routes:
- `/` → `/dashboard` (redirect)
- `/login` – Authentication
- `/dashboard` – System overview
- `/tenants` – Tenant management
- `/adapters` – Adapter management
- `/policies` – Policy configuration
- `/metrics` – System metrics
- `/telemetry` – Event logs
- `/inference` – Inference playground
- `/audit` – Audit logs

## Routing (React Router v6)

- Routes:
  - `/dashboard` – global metrics
  - `/telemetry` – stream viewer
  - `/alerts` – monitoring rules
  - `/replay` – deterministic verification
  - `/policies` – policy/audit views
>

Entry: `src/main.tsx` mounts `BrowserRouter` → `LayoutProvider` → `RootLayout` with `<Outlet>`.

## Legacy routes (MVP)

- Legacy pages (owner/management/personas/flow/lora/trainer/promotion/federation/dev-only) now hard redirect to the core flows (dashboard/training/router-config/system).
- Navigation surfaces only MVP paths: dashboard, adapters, training, router-config, inference, chat, documents, telemetry, routing, replay, testing/golden, metrics/system/observability, base-models, admin/settings.

## Layouts

- `src/layout/RootLayout.tsx` – global shell, safe-area paddings, Toaster at z-40
- `src/layout/FeatureLayout.tsx` – page wrapper with non-overlapping resizable panels, `min-w-0/min-h-0`, persistent split positions

## Z-index tiers

- Controls: z-10
- Dialogs: z-20
- Drawers: z-30
- Toaster/overlays: z-40

## Tech Stack

- **React 18** with TypeScript
- **Vite** for fast development and builds
- **Tailwind CSS** for styling
- **shadcn/ui** components (Radix UI primitives)
- **Lucide React** for icons

## Development

### Quick Start

1. Install dependencies:
   ```bash
   pnpm install
   ```

2. Start development server (full stack - backend + UI):
   ```bash
   pnpm dev
   ```

   This starts:
   - Backend API server on http://localhost:3300
   - React development server on http://localhost:3200
   - Automatic port management and graceful shutdown

3. Open http://localhost:3200 in your browser

### Scripts

- `pnpm dev` - Start full stack development (recommended)
- `pnpm build` - Build for production
- `pnpm test` - Run tests
- `pnpm lint` - Lint code

### Backend Integration

The development server automatically starts the Rust backend. If you need to start backend separately:

```bash
# From project root
cargo run -p adapteros-server -- --config configs/cp.toml
```

Dev config disables PF deny checks and auto-creates the drift baseline on first run. Do not skip these checks in production deployments.


Backend logs: `tail -f server-dev.log` (from pnpm dev) or `server.log`

The dev server runs on http://localhost:3200 and proxies API requests to the backend.
>

### Real MLX backend (Apple Silicon)
- Install MLX: `brew install mlx` (or set `MLX_PATH`/`MLX_INCLUDE_DIR`/`MLX_LIB_DIR`).
- Build backend: `make build-mlx` (features: `multi-backend,mlx`).
- Test/bench: `make test-mlx` / `make bench-mlx`.
- Model: `./scripts/download_model.sh --format mlx --size 32b`; then `export AOS_MODEL_PATH=./var/models/Qwen2.5-7B-Instruct-4bit` and `export AOS_MANIFEST_HASH=756be0c4434c3fe5e1198fcf417c52a662e7a24d0716dbf12aae6246bea84f9e`.
- More detail: `docs/MLX_INSTALLATION_GUIDE.md`, `docs/MLX_INTEGRATION.md`.

### Environment Variables

Create a `.env` file for development:

```bash
# API Configuration
VITE_API_BASE=/api

# Feature Flags
VITE_CHAT_AUTO_LOAD_MODELS=false  # Enable automatic model loading in chat (default: false)
```

Available environment variables:
- `VITE_API_BASE`: API base path (default: `/api`)
- `VITE_API_URL`: Backend API URL (default: `http://127.0.0.1:8080/api`)
- `VITE_SSE_URL`: Server-Sent Events endpoint for real-time updates
- `VITE_METRICS_INTERVAL`: Polling interval in milliseconds (default: `50`)
- `VITE_CHAT_AUTO_LOAD_MODELS`: Auto-load models in chat interface (default: `false`)

For production builds, API calls use relative paths (`/api`). The API client automatically sends credentials with requests for cookie-based authentication.

## Real-time Metrics with SSE
- SSE: Connects to ws://VITE_SSE_URL/metrics?token=JWT for <10ms updates (fallback to polling if WS unavailable).
- Env: VITE_SSE_URL=localhost:8080/v1/stream/metrics, VITE_METRICS_INTERVAL=500 (fallback ms).
- Reconnect: Auto on error/close (backoff 1s-30s).

## Testing

### Unit Tests
Vitest + RTL. Run `pnpm test` or `pnpm test:ui` (coverage lcov).

### Smoke Tests
Run end-to-end smoke tests against a running server:

```bash
# Set server URL if different from localhost:8080
export ADAPTEROS_BASE_URL=http://localhost:8080

# Run smoke tests
../../scripts/ui_smoke.sh
```

Tests key endpoints and verifies basic functionality.

## Building
- Dev: `pnpm dev` (SSE hot-reload).
- Build: `pnpm build` (SSE-enabled, tsc clean).
- Verify: `pnpm test:ui && pnpm build` (coverage + build).

## Recent Fixes
- SSE Integration: subscribeToMetrics in apiClient, replaces polling in RealtimeMetrics (low-latency, reconnect, fallback).
- Tests: +4 SSE tests (connect/parse, disconnect/fallback, 401/reconnect, throttle duplicates). Coverage 55%.
- Clamps: Progress values 0-100 to prevent glitches.

## Environment Variables
- VITE_API_URL: Backend API (default: http://127.0.0.1:8080/api).
- VITE_METRICS_INTERVAL: Polling ms for RealtimeMetrics (default: 50; set to 500 for efficiency).

## Recent Fixes
- RealtimeMetrics.tsx: Added .catch for fetch errors (line 91), fixed comma in objects (114), proper export/brace (401). Now builds without TS errors.
- Tests: ui/src/__tests__/RealtimeMetrics.test.tsx validates rendering, data updates, errors.

## Recent Enhancements (2025-11-13)

### UI Improvements
- **RootLayout.tsx**: Enhanced global layout with improved navigation, safe-area handling, and component integration for better user experience.
- **CoreProviders.tsx**: Updated core providers for enhanced state management, authentication, and API integration.

### Technical Updates
- Improved error handling in API client for better resilience.
- Enhanced real-time metrics with SSE for low-latency updates.
- Expanded test coverage for new components and integrations.

*Last Updated: November 13, 2025*

## Project Structure

```
ui/
├── src/
│   ├── api/
│   │   ├── client.ts      # API client singleton
│   │   └── types.ts       # TypeScript types matching server API
│   ├── components/
│   │   ├── Dashboard.tsx  # Main dashboard view
│   │   ├── Tenants.tsx    # Tenant management
│   │   ├── Nodes.tsx      # Compute node management
│   │   ├── Plans.tsx      # Plan management
│   │   ├── Promotion.tsx  # CP promotion gates
│   │   ├── Telemetry.tsx  # Telemetry bundles
│   │   ├── Policies.tsx   # Policy management
│   │   ├── CodeIntelligence.tsx  # Repository & commit analysis
│   │   └── ui/            # Reusable UI components (shadcn/ui)
│   ├── App.tsx            # Main application component
│   ├── main.tsx           # Application entry point
│   └── index.css          # Global styles
├── index.html
├── vite.config.ts
├── package.json
└── README.md
```

## API Integration

All components use the centralized API client from `src/api/client.ts`:

```typescript
import apiClient from '../api/client';

// Fetch data
const tenants = await apiClient.listTenants();
const metrics = await apiClient.getSystemMetrics();

// Authentication
await apiClient.login({ username, password });
```

The API client automatically:
- Manages JWT tokens in localStorage
- Adds Authorization headers
- Handles errors consistently
- Uses environment-based API URLs

## API Integration Status

- All components now use the centralized `apiClient` for API calls
- ServicePanel and PromptOrchestrationPanel: Raw fetch calls to non-existent endpoints (/api/services, /api/prompt-orchestration) replaced with placeholders using existing apiClient.getStatus()
- Buttons for service control and config save/analysis disabled with informative titles ("under development")
- Error handling standardized with logger.error/toError; no console.error usage
- Verified: No console errors in browser dev tools after fixes

**Completion Date:** [Current Date]
**Status:** Complete - UX fully tied to available API endpoints

## Features

### Authentication
- JWT-based authentication with httpOnly cookies
- Automatic token refresh on 401 responses
- Session persistence via secure cookies
- Route guards with RequireAuth component
- Role-based access control (Admin, Operator, SRE, etc.)

### Dashboard
- Real-time system metrics
- Node health status
- Adapter counts
- Performance metrics

### Management Views
- **Tenants**: Multi-tenant isolation management
- **Nodes**: Compute infrastructure monitoring
- **Plans**: Execution plan compilation
- **Promotion**: Gate-checked control plane promotions
- **Telemetry**: Event bundle export
- **Policies**: Security policy configuration
- **Code Intelligence**: Repository scanning & analysis

## Development Guidelines

### Adding New Components

1. Create component in `src/components/`
2. Import and use API client for data fetching
3. Add TypeScript types from `src/api/types.ts`
4. Follow existing patterns for loading states and error handling
- Dashboard widgets should wrap content in `DashboardWidgetFrame` to standardize title/subtitle, refresh, last-updated label, and loading/error/empty/ready states. Wire `onRefresh` to the relevant React Query `refetch` (or invalidate) and pass `lastUpdated` from the query/polling hook.

```tsx
<DashboardWidgetFrame
  title="Example"
  state={state}
  onRefresh={refetch}
  lastUpdated={lastUpdated}
  emptyMessage="No data"
>
  <WidgetBody />
</DashboardWidgetFrame>
```

## Dashboard widgets

See `docs/ui-component-hierarchy.md#dashboard-widget-pattern` for the required `DashboardWidgetFrame` usage on all dashboard widgets and how to migrate existing widgets when you touch them.

### Adding New API Endpoints

1. Add types to `src/api/types.ts`
2. Add method to API client in `src/api/client.ts`
3. Use in components

### Styling

- Use Tailwind utility classes
- Follow shadcn/ui patterns for consistency
- Use existing design tokens from `globals.css`

## Production Deployment

The UI is built as static files and embedded in the mplora-server binary via `rust-embed`. When the server runs, it serves the UI at the root path (`/`) and APIs at `/api/*`.

```bash
# Full production build
make ui
cargo build --release --bin mplora-server

# Run
./target/release/mplora-server --config configs/cp.toml
# UI available at http://127.0.0.1:8080/
```

## License

Same as parent project (MIT OR Apache-2.0).

MLNavigator Inc 2025-12-08.
