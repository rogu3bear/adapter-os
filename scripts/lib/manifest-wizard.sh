#!/bin/bash
# adapterOS Manifest Wizard
# Interactive wizard for generating configs/manifest.toml on first boot.
# Sources freeze-guard.sh for colors (FG_*).
#
# Usage:
#   source scripts/lib/manifest-wizard.sh
#   run_manifest_wizard [--force] [--profile <dev|production|reference>]
#
# Non-interactive:
#   AOS_MANIFEST_PROFILE=dev ./start init
#
# The generated manifest is a superset of cp.toml — it contains all existing
# config sections plus [instance], [services], [boot], and extended [model].

# Ensure colors are available (freeze-guard.sh should already be sourced)
: "${FG_RED:=\033[0;31m}"
: "${FG_YELLOW:=\033[1;33m}"
: "${FG_GREEN:=\033[0;32m}"
: "${FG_BLUE:=\033[0;34m}"
: "${FG_CYAN:=\033[0;36m}"
: "${FG_BOLD:=\033[1m}"
: "${FG_RESET:=\033[0m}"

# Default manifest path
: "${AOS_MANIFEST_FILE:=configs/manifest.toml}"

# ─── Profile Defaults ────────────────────────────────────────────────────────

# Returns default value for a given profile and key.
# Usage: profile_default <profile> <key>
profile_default() {
    local profile="$1"
    local key="$2"

    case "$profile:$key" in
        # server
        dev:server.production_mode)          echo "false" ;;
        production:server.production_mode)   echo "true" ;;
        reference:server.production_mode)    echo "false" ;;

        dev:server.bind)                     echo "127.0.0.1" ;;
        production:server.bind)              echo "0.0.0.0" ;;
        reference:server.bind)               echo "127.0.0.1" ;;

        # security
        dev:security.dev_bypass)             echo "false" ;;
        production:security.dev_bypass)      echo "false" ;;
        reference:security.dev_bypass)       echo "false" ;;

        dev:security.dev_login_enabled)      echo "true" ;;
        production:security.dev_login_enabled) echo "false" ;;
        reference:security.dev_login_enabled) echo "true" ;;

        # services
        *:services.worker)                   echo "true" ;;

        dev:services.secd)                   echo "false" ;;
        production:services.secd)            echo "true" ;;
        reference:services.secd)             echo "false" ;;

        dev:services.node)                   echo "false" ;;
        production:services.node)            echo "true" ;;
        reference:services.node)             echo "false" ;;

        # boot
        dev:boot.verify_chat)               echo "false" ;;
        production:boot.verify_chat)        echo "true" ;;
        reference:boot.verify_chat)         echo "false" ;;

        dev:boot.health_timeout_secs)       echo "15" ;;
        production:boot.health_timeout_secs) echo "30" ;;
        reference:boot.health_timeout_secs) echo "15" ;;

        dev:boot.readyz_timeout_secs)       echo "30" ;;
        production:boot.readyz_timeout_secs) echo "60" ;;
        reference:boot.readyz_timeout_secs) echo "30" ;;

        # logging
        dev:logging.json_format)            echo "false" ;;
        production:logging.json_format)     echo "true" ;;
        reference:logging.json_format)      echo "false" ;;

        *) echo "" ;;
    esac
}

# ─── Hardware Detection ──────────────────────────────────────────────────────

# Detect hardware and suggest a backend.
# Sets: HW_CHIP, HW_MEMORY_GB, HW_SUGGESTED_BACKEND
detect_hardware() {
    HW_CHIP="unknown"
    HW_MEMORY_GB=0
    HW_SUGGESTED_BACKEND="auto"

    if [ "$(uname -s)" = "Darwin" ]; then
        # macOS: use sysctl
        HW_CHIP="$(sysctl -n machdep.cpu.brand_string 2>/dev/null || echo "unknown")"
        local mem_bytes
        mem_bytes="$(sysctl -n hw.memsize 2>/dev/null || echo 0)"
        HW_MEMORY_GB=$((mem_bytes / 1073741824))

        # Suggest backend based on chip
        case "$HW_CHIP" in
            *Apple*)
                HW_SUGGESTED_BACKEND="mlx"
                ;;
            *)
                HW_SUGGESTED_BACKEND="auto"
                ;;
        esac
    else
        # Linux: /proc
        if [ -f /proc/cpuinfo ]; then
            HW_CHIP="$(grep -m1 'model name' /proc/cpuinfo 2>/dev/null | cut -d: -f2 | xargs || echo "unknown")"
        fi
        if [ -f /proc/meminfo ]; then
            local mem_kb
            mem_kb="$(grep MemTotal /proc/meminfo 2>/dev/null | awk '{print $2}' || echo 0)"
            HW_MEMORY_GB=$((mem_kb / 1048576))
        fi
        HW_SUGGESTED_BACKEND="auto"
    fi
}

# ─── Model Detection ─────────────────────────────────────────────────────────

# Scan var/models/ for available model directories.
# Sets: DETECTED_MODELS (newline-separated list of directory names)
detect_models() {
    DETECTED_MODELS=""
    local models_dir="${AOS_VAR_DIR:-var}/models"

    if [ ! -d "$models_dir" ]; then
        return
    fi

    local found=""
    for dir in "$models_dir"/*/; do
        [ -d "$dir" ] || continue
        local name
        name="$(basename "$dir")"
        # Skip hidden dirs and common non-model dirs
        case "$name" in
            .*|tmp|cache|downloads) continue ;;
        esac
        if [ -n "$found" ]; then
            found="$found"$'\n'"$name"
        else
            found="$name"
        fi
    done
    DETECTED_MODELS="$found"
}

# ─── Interactive Prompts ─────────────────────────────────────────────────────

# Prompt user for a value with a default.
# Usage: prompt_value <var_name> <prompt_text> <default>
prompt_value() {
    local var_name="$1"
    local prompt_text="$2"
    local default="$3"

    if [ -n "$default" ]; then
        printf "  ${FG_CYAN}%s${FG_RESET} [${FG_GREEN}%s${FG_RESET}]: " "$prompt_text" "$default"
    else
        printf "  ${FG_CYAN}%s${FG_RESET}: " "$prompt_text"
    fi

    local input
    read -r input
    input="${input:-$default}"

    eval "$var_name=\"\$input\""
}

# Interactive profile picker.
# Sets: SELECTED_PROFILE
select_profile() {
    echo ""
    echo -e "  ${FG_BOLD}Select a profile:${FG_RESET}"
    echo ""
    echo -e "    ${FG_GREEN}1)${FG_RESET} dev         Local development (default)"
    echo -e "    ${FG_GREEN}2)${FG_RESET} production   Production deployment"
    echo -e "    ${FG_GREEN}3)${FG_RESET} reference    Reference/demo instance"
    echo ""
    printf "  Choice [1]: "

    local choice
    read -r choice
    choice="${choice:-1}"

    case "$choice" in
        1|dev)        SELECTED_PROFILE="dev" ;;
        2|production) SELECTED_PROFILE="production" ;;
        3|reference)  SELECTED_PROFILE="reference" ;;
        *)            SELECTED_PROFILE="dev" ;;
    esac
}

# ─── TOML Writer ─────────────────────────────────────────────────────────────

# Write the manifest TOML file.
# Reads from wizard state variables (M_*).
write_manifest() {
    local path="$1"
    local dir
    dir="$(dirname "$path")"
    mkdir -p "$dir"

    local generated_at
    generated_at="$(date -u +%Y-%m-%dT%H:%M:%SZ)"

    cat > "$path" << MANIFEST_EOF
# adapterOS System Manifest
# Generated by ./start init on $generated_at
# Profile: $M_PROFILE
# Schema version: 1

[instance]
name = "$M_INSTANCE_NAME"
profile = "$M_PROFILE"
generated_at = "$generated_at"
schema_version = 1

[server]
port = $M_PORT
bind = "$M_BIND"
production_mode = $M_PRODUCTION_MODE

[general]
determinism_mode = "besteffort"

[db]
path = "$M_DB_PATH"
storage_mode = "kv_primary"
pool_size = 50
kv_path = "var/aos-kv.redb"
kv_tantivy_path = "var/aos-kv-index"

[auth]
session_lifetime = 43200

[security]
require_pf_deny = false
mtls_required = false
jwt_secret = "$M_JWT_SECRET"
jwt_mode = "$M_JWT_MODE"
dev_login_enabled = $M_DEV_LOGIN_ENABLED
dev_bypass = $M_DEV_BYPASS
allow_registration = true

[paths]
artifacts_root = "var/artifacts"
bundles_root = "var/bundles"
adapters_root = "var/adapters"
plan_dir = "plan"
datasets_root = "var/datasets"
documents_root = "var/documents"

[model]
path = "$M_MODEL_PATH"
backend = "$M_BACKEND"
manifest = "$M_MODEL_MANIFEST"
tokenizer_path = "$M_TOKENIZER_PATH"

[model.cache]
max.mb = 16384

[worker.safety]
inference_timeout_secs = 30
evidence_timeout_secs = 5
router_timeout_ms = 100
policy_timeout_ms = 50
circuit_breaker_threshold = 5
circuit_breaker_timeout_secs = 60
max_concurrent_requests = 20
max_tokens_per_second = 40
max_memory_per_request_mb = 50
max_cpu_time_per_request_secs = 30
max_requests_per_minute = 300
health_check_interval_secs = 30
max_response_time_secs = 60
max_memory_growth_mb = 100
max_cpu_time_secs = 300
max_consecutive_failures = 3

[circuit_breaker]
failure_threshold = 5
reset_timeout_secs = 60
half_open_max_calls = 3
worker_deadline_secs = 600
enable_stub_fallback = true
health_check_interval_secs = 30

[health.adapter]
drift_hard_threshold = 0.15
high_tier_block_threshold = 0.10
deadlock_check_interval_secs = 5
max_wait_time_secs = 30
max_lock_depth = 10
recovery_timeout_secs = 10

[rate_limits]
requests_per_minute = 300
burst_size = 60
inference_per_minute = 150

[capacity_limits]
max_concurrent_training_jobs = 5

[metrics]
enabled = true
bearer_token = "CHANGE_ME_METRICS_TOKEN_32_CHARS_MIN"
include_histogram = true
histogram_buckets = [0.001, 0.005, 0.01, 0.05, 0.1, 0.5, 1.0, 5.0]

[alerting]
enabled = true
alert_dir = "var/alerts"
max_alerts_per_file = 10000
rotate_size_mb = 50

[self_hosting]
mode = "off"
repo_allowlist = []
promotion_threshold = 0.0

[logging]
level = "info"
log_dir = "var/logs"
log_prefix = "aos-cp"
json_format = $M_JSON_FORMAT
rotation = "daily"
max_log_files = 30
include_request_id = true
capture_panics = true

[coreml]
compute_preference = "cpu_and_gpu"
production_mode = false

[model_server]
enabled = false
server_addr = "http://127.0.0.1:18085"
max_kv_cache_sessions = 32
hot_adapter_threshold = 0.10
kv_cache_limit_mb = 0

[services]
worker = $M_SERVICES_WORKER
secd = $M_SERVICES_SECD
node = $M_SERVICES_NODE

[boot]
log_profile = "$M_LOG_PROFILE"
health_timeout_secs = $M_HEALTH_TIMEOUT
readyz_timeout_secs = $M_READYZ_TIMEOUT
auto_seed_model = false
verify_chat = $M_VERIFY_CHAT
MANIFEST_EOF
}

# ─── Main Wizard ─────────────────────────────────────────────────────────────

# Run the manifest wizard.
# Usage: run_manifest_wizard [--force] [--profile <dev|production|reference>]
run_manifest_wizard() {
    local force=0
    local preset_profile=""

    # Parse arguments
    while [ $# -gt 0 ]; do
        case "$1" in
            --force)
                force=1
                ;;
            --profile)
                if [ -n "${2:-}" ]; then
                    preset_profile="$2"
                    shift
                fi
                ;;
            --profile=*)
                preset_profile="${1#*=}"
                ;;
            *)
                ;;
        esac
        shift
    done

    # Check for non-interactive environment variable
    if [ -n "${AOS_MANIFEST_PROFILE:-}" ]; then
        preset_profile="$AOS_MANIFEST_PROFILE"
    fi

    # Check if manifest already exists
    if [ $force -eq 0 ] && manifest_exists "$AOS_MANIFEST_FILE"; then
        echo -e "  ${FG_YELLOW}Manifest already exists:${FG_RESET} $AOS_MANIFEST_FILE"
        echo "  Use --force to regenerate."
        return 0
    fi

    echo ""
    echo -e "${FG_CYAN}━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━${FG_RESET}"
    echo -e "${FG_BOLD}  adapterOS System Manifest Wizard${FG_RESET}"
    echo -e "${FG_CYAN}━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━${FG_RESET}"

    # 1. Profile selection
    local M_PROFILE
    if [ -n "$preset_profile" ]; then
        case "$preset_profile" in
            dev|production|reference) M_PROFILE="$preset_profile" ;;
            *)
                echo -e "  ${FG_RED}Unknown profile: $preset_profile${FG_RESET}"
                echo "  Valid profiles: dev, production, reference"
                return 1
                ;;
        esac
        echo ""
        echo -e "  Profile: ${FG_GREEN}$M_PROFILE${FG_RESET}"
    else
        select_profile
        M_PROFILE="$SELECTED_PROFILE"
        echo -e "  ${FG_GREEN}→${FG_RESET} Using profile: ${FG_BOLD}$M_PROFILE${FG_RESET}"
    fi

    # 2. Detect hardware
    detect_hardware
    echo ""
    echo -e "  ${FG_BOLD}Hardware:${FG_RESET} $HW_CHIP (${HW_MEMORY_GB}GB RAM)"

    # 3. Model path
    detect_models
    local M_MODEL_PATH=""
    local M_MODEL_MANIFEST=""
    local M_TOKENIZER_PATH=""

    if [ -n "$preset_profile" ]; then
        # Non-interactive: use first detected model or default
        if [ -n "$DETECTED_MODELS" ]; then
            local first_model
            first_model="$(echo "$DETECTED_MODELS" | head -1)"
            M_MODEL_PATH="${AOS_VAR_DIR:-var}/models/$first_model"
        else
            M_MODEL_PATH="${AOS_MODEL_PATH:-}"
        fi
    else
        echo ""
        if [ -n "$DETECTED_MODELS" ]; then
            echo -e "  ${FG_BOLD}Detected models:${FG_RESET}"
            local idx=1
            local model_arr=()
            while IFS= read -r model; do
                echo -e "    ${FG_GREEN}$idx)${FG_RESET} $model"
                model_arr+=("$model")
                ((idx++))
            done <<< "$DETECTED_MODELS"
            echo ""
            printf "  Select model [1] or enter path: "
            local model_choice
            read -r model_choice
            model_choice="${model_choice:-1}"

            if [[ "$model_choice" =~ ^[0-9]+$ ]] && [ "$model_choice" -ge 1 ] && [ "$model_choice" -le "${#model_arr[@]}" ]; then
                M_MODEL_PATH="${AOS_VAR_DIR:-var}/models/${model_arr[$((model_choice-1))]}"
            else
                M_MODEL_PATH="$model_choice"
            fi
        else
            prompt_value M_MODEL_PATH "Model path" "${AOS_MODEL_PATH:-}"
        fi
    fi

    # Discover tokenizer alongside model
    if [ -n "$M_MODEL_PATH" ] && [ -f "$M_MODEL_PATH/tokenizer.json" ]; then
        M_TOKENIZER_PATH="$M_MODEL_PATH/tokenizer.json"
    fi

    # Discover manifest alongside model
    if [ -n "$M_MODEL_PATH" ] && [ -f "$M_MODEL_PATH/manifest.json" ]; then
        M_MODEL_MANIFEST="$M_MODEL_PATH/manifest.json"
    fi

    # 4. Backend
    local M_BACKEND="$HW_SUGGESTED_BACKEND"
    if [ -z "$preset_profile" ]; then
        prompt_value M_BACKEND "Backend" "$HW_SUGGESTED_BACKEND"
    fi

    # 5. Port
    local M_PORT=8080
    if [ -z "$preset_profile" ]; then
        prompt_value M_PORT "Server port" "8080"
    fi

    # 6. Services (from profile defaults)
    local M_SERVICES_WORKER
    M_SERVICES_WORKER="$(profile_default "$M_PROFILE" "services.worker")"
    local M_SERVICES_SECD
    M_SERVICES_SECD="$(profile_default "$M_PROFILE" "services.secd")"
    local M_SERVICES_NODE
    M_SERVICES_NODE="$(profile_default "$M_PROFILE" "services.node")"

    if [ -z "$preset_profile" ]; then
        echo ""
        echo -e "  ${FG_BOLD}Services:${FG_RESET}"
        prompt_value M_SERVICES_WORKER "  Start worker" "$M_SERVICES_WORKER"
        prompt_value M_SERVICES_SECD "  Start secd" "$M_SERVICES_SECD"
        prompt_value M_SERVICES_NODE "  Start node" "$M_SERVICES_NODE"
    fi

    # Derive remaining profile defaults
    local M_PRODUCTION_MODE
    M_PRODUCTION_MODE="$(profile_default "$M_PROFILE" "server.production_mode")"
    local M_BIND
    M_BIND="$(profile_default "$M_PROFILE" "server.bind")"
    local M_DEV_BYPASS
    M_DEV_BYPASS="$(profile_default "$M_PROFILE" "security.dev_bypass")"
    local M_DEV_LOGIN_ENABLED
    M_DEV_LOGIN_ENABLED="$(profile_default "$M_PROFILE" "security.dev_login_enabled")"
    local M_VERIFY_CHAT
    M_VERIFY_CHAT="$(profile_default "$M_PROFILE" "boot.verify_chat")"
    local M_HEALTH_TIMEOUT
    M_HEALTH_TIMEOUT="$(profile_default "$M_PROFILE" "boot.health_timeout_secs")"
    local M_READYZ_TIMEOUT
    M_READYZ_TIMEOUT="$(profile_default "$M_PROFILE" "boot.readyz_timeout_secs")"
    local M_JSON_FORMAT
    M_JSON_FORMAT="$(profile_default "$M_PROFILE" "logging.json_format")"
    local M_LOG_PROFILE="json"
    if [ "$M_JSON_FORMAT" = "false" ]; then
        M_LOG_PROFILE="plain"
    fi

    # 7. Production-only: DB and JWT
    local M_DB_PATH="sqlite://var/aos-cp.sqlite3"
    local M_JWT_SECRET="dev-secret-key-for-development-only-32-chars-min-at-least"
    local M_JWT_MODE="hmac"

    if [ "$M_PROFILE" = "production" ]; then
        if [ -z "$preset_profile" ]; then
            echo ""
            echo -e "  ${FG_BOLD}Production settings:${FG_RESET}"
            prompt_value M_DB_PATH "Database URL" "$M_DB_PATH"
            prompt_value M_JWT_MODE "JWT mode (hmac/eddsa)" "eddsa"
            if [ "$M_JWT_MODE" = "eddsa" ]; then
                echo -e "  ${FG_YELLOW}Note:${FG_RESET} Generate Ed25519 key with: openssl genpkey -algorithm Ed25519"
            fi
            prompt_value M_JWT_SECRET "JWT secret/key" ""
        else
            M_JWT_MODE="eddsa"
        fi
    fi

    # Instance name
    local M_INSTANCE_NAME
    if [ -z "$preset_profile" ]; then
        local default_name
        default_name="$(hostname -s 2>/dev/null || echo "adapteros")"
        prompt_value M_INSTANCE_NAME "Instance name" "$default_name"
    else
        M_INSTANCE_NAME="$(hostname -s 2>/dev/null || echo "adapteros")"
    fi

    # 8. Write manifest
    echo ""
    write_manifest "$AOS_MANIFEST_FILE"

    echo -e "  ${FG_GREEN}✓${FG_RESET} Manifest written to ${FG_BOLD}$AOS_MANIFEST_FILE${FG_RESET}"
    echo ""
    echo -e "  Profile:  ${FG_CYAN}$M_PROFILE${FG_RESET}"
    echo -e "  Backend:  ${FG_CYAN}$M_BACKEND${FG_RESET}"
    echo -e "  Port:     ${FG_CYAN}$M_PORT${FG_RESET}"
    if [ -n "$M_MODEL_PATH" ]; then
        echo -e "  Model:    ${FG_CYAN}$M_MODEL_PATH${FG_RESET}"
    fi
    echo ""
    echo -e "  Edit ${FG_BOLD}$AOS_MANIFEST_FILE${FG_RESET} to customize further."
    echo -e "  Run ${FG_BOLD}./start${FG_RESET} to boot with this manifest."
    echo ""
}
