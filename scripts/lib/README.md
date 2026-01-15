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

Port conflict detection and resource management. See `freeze-guard.sh` for details.
