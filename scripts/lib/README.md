# adapterOS Script Libraries

Shared libraries for adapterOS shell scripts.

## env-loader.sh

**Unified environment variable loading and validation.**

### Features

- **Safe .env parsing**: Validates variable names, handles comments, prevents injection
- **Path normalization**: Automatically resolves relative paths relative to script directory
- **Comprehensive validation**: Ports, paths, database URLs, backend selection, security checks
- **Non-override mode**: Respects existing environment variables (config precedence)

### Usage

```bash
# Source the library
source scripts/lib/env-loader.sh

# Set SCRIPT_DIR for path normalization (if needed)
export SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"

# Load .env file
load_env_file ".env" --no-override

# Validate configuration
validate_env_config

# Or quick validation (warnings only)
validate_env_quick
```

### Functions

#### `load_env_file [path] [options]`

Loads environment variables from a file.

**Options:**

- `--strict`: Fail if file doesn't exist
- `--no-override`: Don't override existing variables (default)
- `--override`: Allow overriding existing variables

#### `validate_env_config [--strict]`

Validates environment configuration comprehensively.

#### `validate_env_quick`

Quick validation with non-fatal warnings.

#### `validate_port <port> <name>`

Validates port number (1-65535).

#### `validate_path <path> <name> [required]`

Validates file/directory path existence.

#### `validate_database_url <url>`

Validates database URL format.

#### `validate_backend <backend>`

Validates backend selection (auto, coreml, metal, mlx).

#### `check_env_file [path]`

Checks if .env file exists.

#### `print_env_summary`

Prints a summary of current environment configuration.

### Examples

**Basic usage in a script:**

```bash
#!/bin/bash
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
source "$SCRIPT_DIR/scripts/lib/env-loader.sh"

# Load .env
load_env_file ".env" --no-override

# Validate
if ! validate_env_quick; then
    echo "Configuration errors detected"
    exit 1
fi
```

**With validation:**

```bash
#!/bin/bash
source scripts/lib/env-loader.sh

load_env_file ".env"

if validate_env_config --strict; then
    echo "✓ Configuration valid"
    print_env_summary
else
    echo "✗ Configuration invalid"
    exit 1
fi
```

## freeze-guard.sh

**Port conflict detection and resource management.**

Provides preflight checks for port conflicts, stale PID files, Unix sockets, and database locks. By default, prompts for user confirmation before killing adapterOS processes or removing stale resources. External (non-adapterOS) processes are never touched.

### Environment Variables

- **`FG_AUTO_KILL`**: Set to `1` or `true` to enable non-interactive mode
  - Automatically kills adapterOS processes and cleans up stale resources without prompting
  - **Use case**: Automated environments, CI/CD, agent-driven shells
  - **Default**: Interactive prompts (requires user confirmation)

### Usage

```bash
# Source the library
source scripts/lib/freeze-guard.sh

# Interactive mode (default) - prompts before acting
freeze_check_port 8080 "Backend API"

# Non-interactive mode - auto-kill adapterOS processes
export FG_AUTO_KILL=1
freeze_preflight  # Checks all resources, auto-cleans if needed
```

### Functions

#### `freeze_check_port <port> [service_name]`

Checks if a port is free. If occupied by an adapterOS process, offers to kill it (or auto-kills if `FG_AUTO_KILL=1`). Never touches external processes.

#### `freeze_check_pid_file <pid_file> [service_name]`

Checks for stale PID files and offers to clean them (or auto-cleans if `FG_AUTO_KILL=1`).

#### `freeze_check_socket <socket_path> [service_name]`

Checks for stale Unix sockets and offers to clean them (or auto-cleans if `FG_AUTO_KILL=1`).

#### `freeze_check_db_lock <db_path>`

Detects database locks but does not modify anything (informational only).

#### `freeze_preflight [backend_port] [ui_port] [var_dir]`

Runs all preflight checks before starting adapterOS. Returns 0 if all clear, 1 if blocked.

### Examples

**Interactive mode (manual confirmation):**

```bash
#!/bin/bash
source scripts/lib/freeze-guard.sh

# User will be prompted if port 8080 is in use
if freeze_check_port 8080 "Backend"; then
    echo "Port is free"
else
    echo "Port is blocked"
    exit 1
fi
```

**Non-interactive mode (automated/agent environments):**

```bash
#!/bin/bash
export FG_AUTO_KILL=1  # Enable auto-kill mode
source scripts/lib/freeze-guard.sh

# Will automatically kill adapterOS processes without prompting
freeze_preflight 8080 3200 var
```
