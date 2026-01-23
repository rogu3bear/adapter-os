#!/bin/bash
# adapterOS Unified Environment Loader
# Provides safe .env loading, validation, and helper functions
# Usage: source scripts/lib/env-loader.sh

# Colors (if not already set by freeze-guard.sh)
: "${FG_RED:=\033[0;31m}"
: "${FG_GREEN:=\033[0;32m}"
: "${FG_YELLOW:=\033[1;33m}"
: "${FG_CYAN:=\033[0;36m}"
: "${FG_RESET:=\033[0m}"

# =============================================================================
# Core Loading Functions
# =============================================================================

# Load .env file safely
# Usage: load_env_file [path] [options]
# Options:
#   --strict: Fail if file doesn't exist
#   --no-override: Don't override existing variables
load_env_file() {
    local env_file="${1:-.env}"
    local strict_mode=false
    local no_override=true
    
    # Parse options
    shift || true
    for arg in "$@"; do
        case "$arg" in
            --strict) strict_mode=true ;;
            --no-override) no_override=true ;;
            --override) no_override=false ;;
        esac
    done
    
    if [ ! -f "$env_file" ]; then
        if [ "$strict_mode" = true ]; then
            echo "${FG_RED}✗${FG_RESET} Environment file not found: $env_file" >&2
            return 1
        fi
        return 0
    fi
    
    # Load .env with safe parsing
    set -a
    while IFS= read -r line || [[ -n "$line" ]]; do
        # Skip comments and empty lines
        [[ -z "$line" || "$line" =~ ^[[:space:]]*# ]] && continue
        
        # Only process lines with =
        if [[ "$line" =~ ^[^#]*= ]]; then
            var_name="${line%%=*}"
            # Sanitize variable name (remove spaces, validate)
            var_name="${var_name// /}"
            
            # Validate variable name
            if [[ ! "$var_name" =~ ^[A-Za-z_][A-Za-z0-9_]*$ ]]; then
                echo "${FG_YELLOW}⚠${FG_RESET} Skipping invalid variable name: ${var_name%%=*}" >&2
                continue
            fi
            
            # Don't override existing environment variables if requested
            if [ "$no_override" = true ]; then
                # Check if variable is already set (using eval for compatibility)
                eval "local existing_value=\${$var_name:-}"
                if [ -n "$existing_value" ]; then
                    continue
                fi
            fi
            
            # Export the variable
            eval "export $line" 2>/dev/null || true
        fi
    done < "$env_file"
    set +a
    
    # Normalize paths (resolve relative paths relative to script directory)
    normalize_env_paths
}

# Normalize environment variable paths
normalize_env_paths() {
    local script_dir="${SCRIPT_DIR:-$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)}"
    
    # Normalize AOS_MODEL_PATH
    if [[ -n "${AOS_MODEL_PATH:-}" ]] && [[ ! "${AOS_MODEL_PATH}" =~ ^/ ]]; then
        if [[ -e "$script_dir/$AOS_MODEL_PATH" ]]; then
            export AOS_MODEL_PATH="$script_dir/$AOS_MODEL_PATH"
        fi
    fi
    
    # Normalize AOS_MANIFEST_PATH
    if [[ -n "${AOS_MANIFEST_PATH:-}" ]] && [[ ! "${AOS_MANIFEST_PATH}" =~ ^/ ]]; then
        if [[ -e "$script_dir/$AOS_MANIFEST_PATH" ]]; then
            export AOS_MANIFEST_PATH="$script_dir/$AOS_MANIFEST_PATH"
        fi
    fi
    
    # Normalize AOS_WORKER_MANIFEST
    if [[ -n "${AOS_WORKER_MANIFEST:-}" ]] && [[ ! "${AOS_WORKER_MANIFEST}" =~ ^/ ]]; then
        if [[ -e "$script_dir/$AOS_WORKER_MANIFEST" ]]; then
            export AOS_WORKER_MANIFEST="$script_dir/$AOS_WORKER_MANIFEST"
        fi
    fi
}

# =============================================================================
# Validation Functions
# =============================================================================

# Validate port number
# Returns 0 if valid, 1 if invalid
validate_port() {
    local port="$1"
    local name="${2:-Port}"
    
    if [[ ! "$port" =~ ^[0-9]+$ ]] || [ "$port" -lt 1 ] || [ "$port" -gt 65535 ]; then
        echo "${FG_RED}✗${FG_RESET} $name: Invalid port '$port' (must be 1-65535)" >&2
        return 1
    fi
    return 0
}

# Validate file/directory path
# Usage: validate_path <path> <name> [required]
# Returns 0 if valid/exists, 1 if invalid/missing
validate_path() {
    local path="$1"
    local name="$2"
    local required="${3:-false}"
    local script_dir="${SCRIPT_DIR:-$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)}"
    
    if [ -z "$path" ]; then
        if [ "$required" = "true" ]; then
            echo "${FG_RED}✗${FG_RESET} $name: Not set (required)" >&2
            return 1
        fi
        return 0
    fi
    
    # Expand ~ and resolve relative paths
    local expanded_path="${path/#\~/$HOME}"
    if [[ "$expanded_path" != /* ]]; then
        expanded_path="$script_dir/$expanded_path"
    fi
    
    if [ ! -e "$expanded_path" ]; then
        if [ "$required" = "true" ]; then
            echo "${FG_RED}✗${FG_RESET} $name: Path does not exist '$expanded_path' (required)" >&2
            return 1
        else
            echo "${FG_YELLOW}⚠${FG_RESET} $name: Path does not exist '$expanded_path'" >&2
        fi
    fi
    return 0
}

# Validate database URL
validate_database_url() {
    local url="$1"
    
    if [ -z "$url" ]; then
        echo "${FG_RED}✗${FG_RESET} AOS_DATABASE_URL: Not set (required)" >&2
        return 1
    fi
    
    if [[ "$url" =~ ^sqlite: ]]; then
        local db_path="${url#sqlite:}"
        # Remove // prefix if present
        db_path="${db_path#//}"
        validate_path "$db_path" "Database file" false
    fi
    
    return 0
}

# Validate backend selection
validate_backend() {
    local backend="${1:-auto}"
    
    case "$backend" in
        auto|coreml|metal|mlx)
            return 0
            ;;
        *)
            echo "${FG_YELLOW}⚠${FG_RESET} AOS_MODEL_BACKEND: Unknown backend '$backend' (expected: auto, coreml, metal, mlx)" >&2
            return 0  # Not fatal, just a warning
            ;;
    esac
}

# =============================================================================
# Comprehensive Validation
# =============================================================================

# Validate environment configuration
# Usage: validate_env_config [--strict]
# Returns 0 if valid, 1 if invalid
validate_env_config() {
    local strict_mode=false
    local errors=0
    
    # Parse options
    for arg in "$@"; do
        case "$arg" in
            --strict) strict_mode=true ;;
        esac
    done
    
    # Server configuration
    if [ -n "${AOS_SERVER_PORT:-}" ]; then
        validate_port "${AOS_SERVER_PORT}" "AOS_SERVER_PORT" || ((errors++))
    fi
    
    if [ -n "${AOS_UI_PORT:-}" ]; then
        validate_port "${AOS_UI_PORT}" "AOS_UI_PORT" || ((errors++))
    fi
    
    if [ -n "${AOS_PANEL_PORT:-}" ]; then
        validate_port "${AOS_PANEL_PORT}" "AOS_PANEL_PORT" || ((errors++))
    fi
    
    # Database
    if [ -n "${AOS_DATABASE_URL:-}" ]; then
        validate_database_url "${AOS_DATABASE_URL}" || ((errors++))
    fi
    
    # Model configuration (warnings only, not required for all use cases)
    if [ -n "${AOS_MODEL_PATH:-}" ]; then
        validate_path "${AOS_MODEL_PATH}" "AOS_MODEL_PATH" false || true
    fi
    
    if [ -n "${AOS_MANIFEST_PATH:-}" ]; then
        validate_path "${AOS_MANIFEST_PATH}" "AOS_MANIFEST_PATH" false || true
    fi
    
    if [ -n "${AOS_WORKER_MANIFEST:-}" ]; then
        validate_path "${AOS_WORKER_MANIFEST}" "AOS_WORKER_MANIFEST" false || true
    fi
    
    # Backend validation
    if [ -n "${AOS_MODEL_BACKEND:-}" ]; then
        validate_backend "${AOS_MODEL_BACKEND}" || true
    fi
    
    # Security checks
    if [ "${AOS_SERVER_PRODUCTION_MODE:-false}" = "true" ]; then
        if [ "${AOS_DEV_NO_AUTH:-0}" = "1" ]; then
            echo "${FG_RED}✗${FG_RESET} Security: AOS_DEV_NO_AUTH=1 conflicts with production mode" >&2
            ((errors++))
        fi
        
        if [ -z "${AOS_SECURITY_JWT_SECRET:-}" ] || [[ "${AOS_SECURITY_JWT_SECRET}" =~ ^(test|secret|your-|example) ]]; then
            echo "${FG_RED}✗${FG_RESET} Security: AOS_SECURITY_JWT_SECRET must be set to a secure value in production" >&2
            ((errors++))
        fi
    fi
    
    return $errors
}

# Quick validation (non-fatal warnings)
validate_env_quick() {
    local errors=0
    
    # Validate port
    if [ -n "${AOS_SERVER_PORT:-}" ]; then
        validate_port "${AOS_SERVER_PORT}" "AOS_SERVER_PORT" || ((errors++))
    fi
    
    # Warn if model path is set but doesn't exist
    if [ -n "${AOS_MODEL_PATH:-}" ] && [ ! -e "$AOS_MODEL_PATH" ]; then
        echo "${FG_YELLOW}⚠${FG_RESET} Model path not found: $AOS_MODEL_PATH (worker may fail to start)" >&2
    fi
    
    # Security check
    if [ "${AOS_SERVER_PRODUCTION_MODE:-false}" = "true" ] && [ "${AOS_DEV_NO_AUTH:-0}" = "1" ]; then
        echo "${FG_RED}✗${FG_RESET} Security violation: AOS_DEV_NO_AUTH=1 conflicts with production mode" >&2
        ((errors++))
    fi
    
    return $errors
}

# =============================================================================
# Helper Functions
# =============================================================================

# Print environment summary
print_env_summary() {
    echo "Environment configuration:"
    echo "  Server: ${AOS_SERVER_HOST:-127.0.0.1}:${AOS_SERVER_PORT:-8080}"
    echo "  Database: ${AOS_DATABASE_URL:-not set}"
    echo "  Model: ${AOS_MODEL_PATH:-not set}"
    echo "  Backend: ${AOS_MODEL_BACKEND:-auto}"
    if [ "${AOS_DEV_NO_AUTH:-0}" = "1" ]; then
        echo -e "  Mode: ${FG_YELLOW}Development${FG_RESET} (auth bypass enabled)"
    elif [ "${AOS_SERVER_PRODUCTION_MODE:-false}" = "true" ]; then
        echo -e "  Mode: ${FG_GREEN}Production${FG_RESET}"
    else
        echo "  Mode: Development"
    fi
}

# Check if .env file exists
check_env_file() {
    local env_file="${1:-.env}"
    
    if [ ! -f "$env_file" ]; then
        echo "${FG_RED}✗${FG_RESET} Environment file not found: $env_file" >&2
        echo "  Run: cp .env.example .env" >&2
        return 1
    fi
    return 0
}
