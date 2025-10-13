# AdapterOS Control Plane UI

Modern React-based web interface for managing the AdapterOS control plane.

## Tech Stack

- **React 18** with TypeScript
- **Vite** for fast development and builds
- **Tailwind CSS** for styling
- **shadcn/ui** components (Radix UI primitives)
- **Lucide React** for icons

## Development

### Prerequisites

- Node.js 20+
- pnpm (preferred package manager)

### Setup

```bash
# Install dependencies
pnpm install

# Start development server
pnpm dev
```

The dev server runs on http://localhost:3200 and proxies API requests to the backend.

### Environment Variables

Create a `.env.local` file for development:

```bash
VITE_API_URL=http://127.0.0.1:8080/api
```

For production builds, API calls use relative paths (`/api`).

## Building

### Production Build

```bash
# Build for production
pnpm build

# Output: ../crates/mplora-server/static/
```

The build output is configured to be embedded directly in the mplora-server binary.

### From Project Root

```bash
# Build UI using Makefile
make ui

# Start dev server
make ui-dev
```

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

## Features

### Authentication
- JWT-based authentication
- Persistent sessions via localStorage
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