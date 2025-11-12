# 🚀 AdapterOS Launch Panel

## Single Command to Launch Everything

The **Launch Panel** is your one-stop command to spin up the entire AdapterOS system. No more remembering which services to start in which order - just run one command and you're ready to go!

## 🎯 Quick Start

```bash
# Launch everything with one command
./launch.sh

# Or use the alias
./aos-launch
```

That's it! The launch panel will:
- ✅ Run pre-flight checks
- ✅ Start services in the correct order
- ✅ Wait for everything to be ready
- ✅ Show you access URLs
- ✅ Monitor system health
- ✅ Handle graceful shutdown

## 📋 Available Commands

| Command | Description |
|---------|-------------|
| `./launch.sh` | Launch full system (backend + UI + menu bar) |
| `./launch.sh backend` | Launch backend API server only |
| `./launch.sh ui` | Launch backend + web UI only |
| `./launch.sh status` | Show current service status |
| `./launch.sh stop [mode]` | Stop all running services (graceful|fast|immediate) |
| `./launch.sh help` | Show detailed help |

## 🌐 Access URLs

After launching, you'll have access to:

- **Backend API**: http://localhost:3300
- **Web Dashboard**: http://localhost:3200
- **Health Check**: `curl http://localhost:3300/healthz`
- **API Documentation**: http://localhost:3300/docs

## 🛠️ Service Management

For more granular control, use the service manager:

```bash
# Check status
./aos status

# Start/stop individual services
./aos start backend
./aos stop ui
./aos restart all

# View logs
./aos logs backend
```

## 🛑 Graceful Shutdown

AdapterOS includes a comprehensive graceful shutdown system with multiple modes:

### Shutdown Modes

- **Graceful** (default): Full phased shutdown with cleanup
  - Drains connections (10s)
  - Critical cleanup: flush telemetry, save state (30s)
  - Resource cleanup: unload models/adapters (60s)
  - Final cleanup: close databases (10s)
  - Total timeout: ~120s

- **Fast**: Skips drain phase, goes straight to critical cleanup
  - Useful when you need to stop quickly but still want cleanup

- **Immediate**: Minimal cleanup, force stops everything
  - Use only in emergencies

### Usage

```bash
# Graceful shutdown (default)
./launch.sh stop
./scripts/graceful-shutdown.sh graceful

# Fast shutdown
./launch.sh stop fast
./scripts/graceful-shutdown.sh fast

# Immediate shutdown
./launch.sh stop immediate
./scripts/graceful-shutdown.sh immediate

# Or use the service manager
./scripts/service-manager.sh stop all graceful
./scripts/service-manager.sh stop all fast
./scripts/service-manager.sh stop all immediate
```

### How It Works

1. **Ctrl+C in launch script**: Automatically triggers graceful shutdown
2. **Backend server**: Supports SIGTERM (graceful), SIGUSR1 (fast), SIGUSR2 (immediate)
3. **Service order**: Stops UI → Menu Bar → Backend (reverse of startup)
4. **Timeouts**: Each service has appropriate timeouts with fallback to force kill
5. **Status feedback**: Real-time progress and error reporting

The shutdown script checks system readiness (active requests, training jobs, loaded models) and adjusts behavior accordingly.

## 🔧 What the Launch Panel Does

### Pre-flight Checks
- Verifies you're in the right directory
- Checks if backend binary exists (builds if needed)
- Verifies ports are available
- Validates configuration files

### Service Startup Sequence
1. **Backend Server** (Port 3300) - Core API with rate limiting fix
2. **Web UI** (Port 3200) - React dashboard
3. **Menu Bar App** (macOS) - System tray status monitor

### Health Monitoring
- Waits for services to be ready
- Periodic status checks every 30 seconds
- Graceful error handling

### Shutdown Handling
- Press `Ctrl+C` to stop everything cleanly
- Automatic cleanup of processes
- Status preservation

## 🚨 Troubleshooting

### If Launch Fails
```bash
# Check what's running
./launch.sh status

# Stop everything and try again
./launch.sh stop
./launch.sh

# Check logs
./aos logs backend
./aos logs ui
```

### Port Conflicts
If ports 3300 or 3200 are in use:
```bash
# Find what's using the ports
lsof -i :3300
lsof -i :3200

# Kill conflicting processes or change ports in configs/cp.toml
```

### Build Issues
If the backend won't start:
```bash
# Rebuild everything
cargo clean
cargo build

# Then launch
./launch.sh
```

## 🎨 Features

- **Beautiful Output**: Color-coded status messages
- **Smart Dependencies**: Starts services in the right order
- **Health Checks**: Verifies services are actually working
- **Error Recovery**: Continues with partial failures
- **Status Monitoring**: Real-time system health
- **Graceful Shutdown**: Clean process termination

## 🔒 Security Notes

- Backend runs with authentication required
- Rate limiting protects against abuse
- Services bind to localhost only by default
- No external network exposure

## Development Setup

### Full Stack Development

To start the full development environment (backend server + React UI) with automatic port management and graceful shutdown:

```bash
cd ui
pnpm dev
```

This command:
- Ensures ports 3300 (backend) and 3200 (UI) are available
- Starts the Rust backend server (compiles if needed)
- Starts the Vite development server for the UI
- Waits for backend to be ready before starting UI
- Handles graceful shutdown on Ctrl+C (sends SIGTERM to backend, waits 30s, force kills if needed)
- Logs backend output to `server-dev.log` in project root

### Individual Components

- **Backend only**: `./launch.sh backend`
- **UI only**: `cd ui && pnpm dev` (now starts full stack by default)
- **Status check**: `./launch.sh status` or visit http://localhost:3301 (service panel if running)

### Logs

- Backend: `tail -f server-dev.log` or `tail -f server.log`
- UI: `tail -f ui-dev.log`
- Service Panel: Console output on port 3301

---

**🎉 Ready to launch? Just run `./launch.sh` and you're all set!**
