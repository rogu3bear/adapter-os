# aos-launch

**Status:** Canonical orchestration script for local development and testing
**Location:** `./aos-launch` (root directory)

## Purpose

`aos-launch` is the recommended way to start the full AdapterOS stack locally. It provides comprehensive pre-flight checks, automatic dependency management, port conflict resolution, and coordinated startup of all services.

## Key Features

### 1. Pre-Flight Checks
- Verifies you're in the correct directory (`configs/cp.toml` exists)
- Checks for required binaries (`adapteros-server`)
- Automatically builds backend if missing
- Initializes database if not present (`var/aos-cp.sqlite3`)
- Checks for `pnpm` or `npm` for UI builds

### 2. Port Management
- Checks port availability:
  - **3300** - Backend API
  - **3200** - Web UI
- Detects and offers to kill conflicting processes
- Includes safety checks to avoid killing system-critical processes

### 3. Service Orchestration
- **Backend Server:**
  - Starts `adapteros-server` with appropriate backend (Metal/MLX)
  - Waits for HTTP response on `/v1/meta` or `/healthz`
  - Monitors for successful startup
- **Web Dashboard:**
  - Starts development server (`pnpm dev`)
  - Serves UI on port 3200
- **Menu Bar App (macOS):**
  - Launches native menu bar application
  - Provides quick access to system status

### 4. Health Monitoring
- Periodic status checks every 30 seconds
- Graceful shutdown on Ctrl+C (SIGINT/SIGTERM)
- Ensures all child processes are terminated cleanly

### 5. Backend Selection
- **Metal (default):** GPU-accelerated inference on macOS
- **MLX:** Alternative backend via Python/MLX FFI

## Usage

### Basic Usage

```bash
# Start all services (backend + UI + menu bar)
./aos-launch

# Start only backend
./aos-launch backend

# Start only UI
./aos-launch ui

# Start only menu bar app
./aos-launch menubar
```

### Backend Selection

```bash
# Default: Metal backend
./aos-launch

# MLX backend (requires AOS_MLX_FFI_MODEL environment variable)
export AOS_MLX_FFI_MODEL=/path/to/model
./aos-launch mlx
```

### Advanced Options

```bash
# Set custom ports (modify aos-launch script or use environment variables)
BACKEND_PORT=3301 UI_PORT=3201 ./aos-launch

# Run with verbose logging
AOS_LOG=debug ./aos-launch
```

## Architecture

```
aos-launch
├── Pre-flight Checks
│   ├── Directory verification
│   ├── Binary checks
│   ├── Database initialization
│   └── Dependency checks (pnpm/npm)
│
├── Port Management
│   ├── Port availability checks
│   ├── Conflict detection
│   └── Process cleanup
│
├── Service Startup
│   ├── Backend Server (adapteros-server)
│   ├── Web Dashboard (pnpm dev)
│   └── Menu Bar App (macOS)
│
└── Health Monitoring
    ├── Periodic status checks
    ├── Graceful shutdown handler
    └── Child process cleanup
```

## Health Check Endpoints

The backend server exposes health check endpoints that `aos-launch` monitors:

- **`/v1/meta`** - Returns metadata including version and status
- **`/healthz`** - Simple health check endpoint

`aos-launch` polls these endpoints during startup to verify the backend is ready.

## Graceful Shutdown

When you press Ctrl+C or send SIGTERM, `aos-launch`:
1. Traps the signal
2. Sends SIGTERM to all child processes
3. Waits for graceful shutdown
4. Cleans up any remaining processes
5. Exits with appropriate status code

## Environment Variables

### Backend Selection
- `AOS_MLX_FFI_MODEL` - Path to MLX model (enables MLX backend)

### Logging
- `AOS_LOG` - Log level (trace, debug, info, warn, error)
- `RUST_LOG` - Rust-specific logging configuration

### Ports (modifiable in script)
- `BACKEND_PORT` - Backend API port (default: 3300)
- `UI_PORT` - Web UI port (default: 3200)

## Troubleshooting

### Port Conflicts

If you see "Port 3300 already in use":
1. `aos-launch` will detect the conflict
2. It will show the process using the port
3. You can choose to kill it or change ports

### Database Issues

If database initialization fails:
```bash
# Manually initialize
./target/release/aosctl db migrate

# Or remove and reinitialize
rm var/aos-cp.sqlite3
./aos-launch
```

### Backend Build Failures

If the backend fails to build:
```bash
# Build manually
cargo build --release --bin adapteros-server

# Then retry aos-launch
./aos-launch
```

### MLX Backend Not Starting

If MLX backend fails:
1. Ensure `AOS_MLX_FFI_MODEL` is set correctly
2. Verify the model file exists
3. Check that MLX dependencies are installed

## When NOT to Use aos-launch

- **Production deployments** - Use systemd/launchd services with `aosctl`
- **CI/CD pipelines** - Use individual service commands
- **Cluster environments** - Use orchestration tools (Kubernetes, etc.)
- **Simple service control** - Use `aos` for quick start/stop/restart

For production, prefer:
```bash
# Production database setup
aosctl db migrate

# Production deployment
aosctl deploy adapters --adapters-root /var/adapters

# Service control
systemctl start adapteros-server
```

## Related Commands

- [`aos`](./aos.md) - Simple service control (start/stop/restart/status)
- [`aosctl`](./aosctl.md) - System administration and operations
- [`cargo xtask`](./xtask.md) - Developer automation tasks

## Implementation Details

**Location:** `/Users/star/Dev/adapter-os/aos-launch`
**Lines of Code:** ~403
**Language:** Bash
**Dependencies:** bash, lsof (for port checks), cargo, pnpm or npm

## Source

For the complete implementation, see: `aos-launch:1-403`
