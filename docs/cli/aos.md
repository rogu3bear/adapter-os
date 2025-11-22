# aos

**Status:** Active Rust CLI for local service control
**Location:** `crates/adapteros-aos/src/bin/aos.rs`
**Installation:** `cargo install --path crates/adapteros-aos` or included in release builds

## Purpose

`aos` provides simple, direct control over local AdapterOS services. It's designed for quick operations on a single node without database or cluster coordination.

## Commands

### `aos start [service]`

Start one or more services.

```bash
# Start all services
aos start

# Start specific service
aos start backend
aos start ui
aos start menubar
```

**Options:**
- `--config <path>` - Custom configuration file (default: `configs/cp.toml`)
- `--dry-run` - Show what would be started without actually starting
- `--json` - Output in JSON format

### `aos stop [service]`

Stop one or more services.

```bash
# Stop all services
aos stop

# Stop specific service
aos stop backend
```

**Options:**
- `--config <path>` - Custom configuration file
- `--json` - Output in JSON format

### `aos restart [service]`

Restart one or more services (stop then start).

```bash
# Restart all services
aos restart

# Restart specific service
aos restart backend
```

### `aos status [service]`

Show status of services.

```bash
# Status of all services
aos status

# Status of specific service
aos status backend
```

**Options:**
- `--json` - Output in JSON format (structured status information)

### `aos logs [service]`

View service logs.

```bash
# View all logs
aos logs

# View specific service logs
aos logs backend
aos logs ui
```

**Options:**
- `--follow, -f` - Follow log output (tail -f behavior)
- `--lines, -n <num>` - Number of lines to show (default: 100)

## Configuration

`aos` reads configuration from `configs/cp.toml` by default. You can override this with the `--config` flag.

Example configuration:
```toml
[server]
bind = "127.0.0.1:3300"
production_mode = false

[backend]
type = "metal"  # or "mlx"
```

## Telemetry

All `aos` operations emit telemetry events with:
- `component="aos"`
- `service="backend"|"ui"|"menubar"`
- `action="start"|"stop"|"restart"|"status"`
- `result="success"|"error"`

Events are logged with structured JSON for observability.

## When to Use `aos` vs `aos-launch`

**Use `aos` when:**
- You need quick service control (start/stop/restart)
- You're already set up and just need to control services
- You want minimal overhead

**Use `aos-launch` when:**
- You're starting from scratch
- You need pre-flight checks and automatic setup
- You want port conflict detection
- You need health monitoring

## Examples

### Start the backend in development mode
```bash
aos start backend
```

### Restart all services after a configuration change
```bash
aos restart
```

### Check if backend is running
```bash
aos status backend --json
```

### View backend logs in real-time
```bash
aos logs backend --follow
```

### Stop everything before a manual build
```bash
aos stop
cargo build --release
aos start
```

## Exit Codes

- **0** - Success
- **1** - General error
- **2** - Service not found
- **3** - Service already running (for start)
- **4** - Service not running (for stop)

## Related Commands

- [`aos-launch`](./AOS-LAUNCH.md) - Full orchestration with pre-flight checks
- [`aosctl`](./AOSCTL.md) - System administration and database operations
- [`cargo xtask`](./XTASK.md) - Developer automation

## Source

For implementation details, see: `crates/adapteros-aos/src/bin/aos.rs:1-260`
