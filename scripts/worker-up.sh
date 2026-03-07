#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
ROOT_DIR="$(cd "$SCRIPT_DIR/.." && pwd)"
source "$ROOT_DIR/scripts/lib/build-targets.sh"
source "$ROOT_DIR/scripts/lib/model-config.sh"
cd "$ROOT_DIR"

die() {
    echo "ERROR: $*" >&2
    exit 1
}

info() {
    echo ""
    echo "== $* =="
}

print_kv() {
    printf "  %-20s %s\n" "$1" "$2"
}

print_env_if_set() {
    local key="$1"
    local val="${!key:-}"
    if [ -n "$val" ]; then
        print_kv "$key" "$val"
    fi
}

require_cmd() {
    local cmd="$1"
    local hint="${2:-}"
    if ! command -v "$cmd" >/dev/null 2>&1; then
        if [ -n "$hint" ]; then
            die "Missing '$cmd'. $hint"
        fi
        die "Missing '$cmd'."
    fi
}

read_server_port() {
    local config_path="$1"
    local port
    port=$(
        awk '
            $0 ~ /^\[server\]/ { in_server = 1; next }
            in_server && $0 ~ /^\[/ { in_server = 0 }
            in_server && $0 ~ /^[[:space:]]*port[[:space:]]*=/ {
                gsub(/#.*/, "", $0)
                split($0, a, "=")
                gsub(/[[:space:]]/, "", a[2])
                print a[2]
                exit
            }
        ' "$config_path"
    )
    if [ -n "$port" ]; then
        echo "$port"
    else
        echo "8080"
    fi
}

read_db_url() {
    local config_path="$1"
    local db_url="${AOS_DATABASE_URL:-}"
    if [ -n "$db_url" ]; then
        echo "$db_url"
        return 0
    fi
    db_url=$(
        awk '
            $0 ~ /^\[db\]/ { in_db = 1; next }
            in_db && $0 ~ /^\[/ { in_db = 0 }
            in_db && $0 ~ /^[[:space:]]*path[[:space:]]*=/ {
                gsub(/#.*/, "", $0)
                split($0, a, "=")
                gsub(/^[[:space:]]+|[[:space:]]+$/, "", a[2])
                gsub(/"/, "", a[2])
                print a[2]
                exit
            }
        ' "$config_path"
    )
    if [ -n "$db_url" ]; then
        echo "$db_url"
    fi
}

read_db_path() {
    local config_path="$1"
    local db_url
    db_url="$(read_db_url "$config_path")"
    if [ -z "$db_url" ]; then
        return 0
    fi
    case "$db_url" in
        sqlite://*)
            local db_path="${db_url#sqlite://}"
            if [[ "$db_path" != /* ]]; then
                db_path="$ROOT_DIR/$db_path"
            fi
            echo "$db_path"
            ;;
        *)
            ;;
    esac
}

ensure_dev_db_prereqs() {
    local db_path="$1"
    if [ -z "$db_path" ]; then
        return 0
    fi
    if [ ! -f "$db_path" ]; then
        die "Database not found: $db_path. Run ./scripts/dev-up.sh to initialize."
    fi
    require_cmd sqlite3 "Install sqlite3 (macOS: brew install sqlite)."

    local tenant_exists
    tenant_exists="$(sqlite3 "$db_path" "select 1 from tenants where id='default' limit 1;" 2>/dev/null || true)"
    if [ "$tenant_exists" != "1" ]; then
        sqlite3 "$db_path" "insert into tenants (id, name, itar_flag, status) values ('default','Default',0,'active');" 2>/dev/null || true
        tenant_exists="$(sqlite3 "$db_path" "select 1 from tenants where id='default' limit 1;" 2>/dev/null || true)"
        if [ "$tenant_exists" != "1" ]; then
            die "Default tenant missing in $db_path. Insert: sqlite3 $db_path \"insert into tenants (id,name,itar_flag,status) values ('default','Default',0,'active');\""
        fi
        print_kv "db_fix" "created tenant default"
    fi

    local node_exists
    node_exists="$(sqlite3 "$db_path" "select 1 from nodes where id='local' limit 1;" 2>/dev/null || true)"
    if [ "$node_exists" != "1" ]; then
        sqlite3 "$db_path" "insert or ignore into nodes (id, hostname, agent_endpoint, status, created_at) values ('local','local','http://127.0.0.1:8081','active', datetime('now'));" 2>/dev/null || true
        node_exists="$(sqlite3 "$db_path" "select 1 from nodes where id='local' limit 1;" 2>/dev/null || true)"
        if [ "$node_exists" != "1" ]; then
            die "Local node missing in $db_path. Insert: sqlite3 $db_path \"insert into nodes (id,hostname,agent_endpoint,status,created_at) values ('local','local','http://127.0.0.1:8081','active', datetime('now'));\""
        fi
        print_kv "db_fix" "created node local"
    fi
}

abspath() {
    local path="$1"
    if [[ "$path" != /* ]]; then
        echo "$ROOT_DIR/$path"
    else
        echo "$path"
    fi
}

lower() {
    echo "$1" | tr '[:upper:]' '[:lower:]'
}

print_cmd() {
    printf "+ "
    printf "%q " "$@"
    echo ""
}

is_truthy() {
    local raw="${1:-}"
    local val
    val="$(echo "$raw" | tr '[:upper:]' '[:lower:]')"
    case "$val" in
        1|true|yes|on) return 0 ;;
        0|false|no|off|"") return 1 ;;
        *) return 1 ;;
    esac
}

list_model_dirs() {
    local root="$1"
    if [ ! -d "$root" ]; then
        return 1
    fi
    find "$root" -maxdepth 1 -mindepth 1 -type d 2>/dev/null
}

select_default_model_dir() {
    local root="$1"
    local candidates=()
    local path
    while IFS= read -r path; do
        candidates+=("$path")
    done < <(list_model_dirs "$root" || true)

    if [ "${#candidates[@]}" -eq 0 ]; then
        return 1
    fi

    local name
    for path in "${candidates[@]}"; do
        name="$(lower "$(basename "$path")")"
        if [[ "$name" == *"mistral"* && "$name" == *"7b"* ]]; then
            echo "$path"
            return 0
        fi
    done
    for path in "${candidates[@]}"; do
        name="$(lower "$(basename "$path")")"
        if [[ "$name" == *"0.5b"* || "$name" == *"0_5b"* ]]; then
            if [[ "$path" == *.mlpackage ]]; then
                echo "$path"
                return 0
            fi
        fi
    done

    for path in "${candidates[@]}"; do
        name="$(lower "$(basename "$path")")"
        if [[ "$name" == *"0.5b"* || "$name" == *"0_5b"* ]]; then
            echo "$path"
            return 0
        fi
    done

    local best=""
    local best_size=""
    local size
    for path in "${candidates[@]}"; do
        size="$(du -sk "$path" 2>/dev/null | awk '{print $1}')"
        if [ -z "$best" ] || [ "$size" -lt "$best_size" ]; then
            best="$path"
            best_size="$size"
        fi
    done

    if [ -n "$best" ]; then
        echo "$best"
        return 0
    fi
    return 1
}

select_embedding_model_dir() {
    local model_root="$1"
    local candidates=()
    local path
    while IFS= read -r path; do
        if [ -f "$path/model.safetensors" ] || [ -f "$path/model.safetensors.index.json" ]; then
            candidates+=("$path")
        fi
    done < <(find -L "$model_root" -maxdepth 1 -mindepth 1 -type d 2>/dev/null)

    if [ "${#candidates[@]}" -eq 0 ]; then
        return 1
    fi

    local name
    for path in "${candidates[@]}"; do
        name="$(lower "$(basename "$path")")"
        if [[ "$name" == *"0.5b"* || "$name" == *"0_5b"* ]]; then
            echo "$path"
            return 0
        fi
    done

    echo "${candidates[0]}"
    return 0
}

infer_backend_for_model() {
    local model_path="$1"
    if [[ "$model_path" == *.mlpackage ]]; then
        echo "coreml"
    else
        echo "mlx"
    fi
}

select_manifest_path() {
    local model_path="$1"
    local backend="$2"
    local name
    name="$(lower "$(basename "$model_path")")"

    if [[ "$name" == *"0.5b"* || "$name" == *"0_5b"* ]]; then
        if [ "$backend" = "coreml" ]; then
            echo "$ROOT_DIR/manifests/qwen0.5b-coreml.yaml"
        else
            echo "$ROOT_DIR/manifests/qwen0.5b-safetensors.yaml"
        fi
        return 0
    fi

    if [[ "$name" == *"mistral"* ]]; then
        if [[ "$name" == *"7b"* && "$name" == *"4bit"* ]]; then
            echo "$ROOT_DIR/manifests/mistral7b-4bit-mlx.yaml"
            return 0
        fi
        if [[ "$name" == *"7b"* ]]; then
            echo "$ROOT_DIR/manifests/mistral7b-4bit-mlx.yaml"
            return 0
        fi
    fi

    if [[ "$name" == *"7b"* && "$name" == *"4bit"* ]]; then
        echo "$ROOT_DIR/manifests/qwen7b-4bit-mlx.yaml"
        return 0
    fi

    if [[ "$name" == *"7b"* ]]; then
        echo "$ROOT_DIR/manifests/qwen7b-mlx.yaml"
        return 0
    fi

    return 1
}

info "Preflight"
if [ ! -f "$ROOT_DIR/Cargo.toml" ] || [ ! -d "$ROOT_DIR/crates/adapteros-lora-worker" ]; then
    die "Run from the adapter-os repo root (missing Cargo.toml or crates/adapteros-lora-worker)."
fi

require_cmd curl "Install curl (macOS: brew install curl)."
require_cmd lsof "Install lsof (macOS: brew install lsof)."

print_env_if_set "AOS_DEV_NO_AUTH"
print_env_if_set "AOS_CONFIG"
print_env_if_set "AOS_WORKER_MANIFEST"
print_env_if_set "AOS_MANIFEST_PATH"
print_env_if_set "AOS_MANIFEST_HASH"
print_env_if_set "AOS_DATABASE_URL"
print_env_if_set "AOS_MODEL_PATH"
print_env_if_set "AOS_TOKENIZER_PATH"
print_env_if_set "AOS_EMBEDDING_MODEL_PATH"
print_env_if_set "AOS_MODEL_BACKEND"
print_env_if_set "AOS_WORKER_SOCKET"
print_env_if_set "AOS_CP_URL"

if is_truthy "${AOS_DEV_NO_AUTH:-}"; then
    echo "WARN: AOS_DEV_NO_AUTH=1 disables auth. Worker registration will not include credentials."
fi
WORKER_TARGET_DIR="$(aos_export_cargo_target worker)"
aos_prune_incremental_if_needed worker || true
aos_print_build_context worker

CONFIG_PATH="${AOS_CONFIG:-$ROOT_DIR/configs/cp.toml}"
if [ ! -f "$CONFIG_PATH" ]; then
    die "Config not found: $CONFIG_PATH"
fi
print_kv "config_path" "$CONFIG_PATH"

DB_PATH="$(read_db_path "$CONFIG_PATH")"
if [ -n "$DB_PATH" ]; then
    ensure_dev_db_prereqs "$DB_PATH"
fi

aos_resolve_model_runtime_env "$ROOT_DIR"
model_path="${AOS_MODEL_PATH}"
backend="${AOS_MODEL_BACKEND:-}"
if [ -z "$backend" ]; then
    backend="$(infer_backend_for_model "$model_path")"
fi
if [ "$backend" = "mock" ]; then
    echo "NOTE: using mock backend with the debug worker binary (release builds still reject mock)."
fi

manifest_path="${AOS_WORKER_MANIFEST:-${AOS_MANIFEST_PATH:-}}"
if [ -z "$manifest_path" ]; then
    manifest_path="$(aos_guess_manifest_path "$ROOT_DIR" "${AOS_BASE_MODEL_ID:-}" "$model_path" || true)"
fi
if [ -z "$manifest_path" ]; then
    manifest_path="$(select_manifest_path "$model_path" "$backend" || true)"
    if [ -z "$manifest_path" ]; then
        die "No default manifest for model_path=$model_path. Set AOS_WORKER_MANIFEST (and optional AOS_MANIFEST_HASH)."
    fi
fi
manifest_hash="${AOS_MANIFEST_HASH:-}"

tokenizer_path="${AOS_TOKENIZER_PATH:-}"
if [ -z "$tokenizer_path" ] && [ -d "$model_path" ]; then
    tokenizer_path="$model_path/tokenizer.json"
fi

embedding_model_path="${AOS_EMBEDDING_MODEL_PATH:-}"
if [ -z "$embedding_model_path" ] && [ "$backend" = "coreml" ]; then
    embedding_model_path="$(select_embedding_model_dir "$ROOT_DIR/var/models" || true)"
fi

manifest_path="$(abspath "$manifest_path")"
model_path="$(abspath "$model_path")"
if [ -n "$tokenizer_path" ]; then
    tokenizer_path="$(abspath "$tokenizer_path")"
fi
if [ -n "$embedding_model_path" ]; then
    embedding_model_path="$(abspath "$embedding_model_path")"
    export AOS_EMBEDDING_MODEL_PATH="$embedding_model_path"
fi

uds_path="${AOS_WORKER_SOCKET:-$ROOT_DIR/var/run/dev-worker.sock}"
uds_path="$(abspath "$uds_path")"
case "$uds_path" in
    "$ROOT_DIR"/var/run/*) ;;
    *) die "AOS_WORKER_SOCKET must be under $ROOT_DIR/var/run (got $uds_path)." ;;
esac

print_kv "manifest_path" "$manifest_path"
if [ -n "$manifest_hash" ]; then
    print_kv "manifest_hash" "$manifest_hash"
else
    print_kv "manifest_hash" "<auto>"
fi
print_kv "model_path" "$model_path"
print_kv "tokenizer_path" "${tokenizer_path:-<auto>}"
print_kv "embedding_model" "${AOS_EMBEDDING_MODEL_PATH:-<unset>}"
print_kv "backend" "$backend"
print_kv "uds_path" "$uds_path"

if [ ! -f "$manifest_path" ]; then
    die "Manifest not found: $manifest_path. Set AOS_WORKER_MANIFEST or AOS_MANIFEST_PATH."
fi

if [ ! -d "$model_path" ]; then
    die "Model directory not found: $model_path. Run ./scripts/download-model.sh or set AOS_MODEL_PATH."
fi

if [ -n "${AOS_TOKENIZER_PATH:-}" ] && [ ! -f "$tokenizer_path" ]; then
    die "Tokenizer not found: $tokenizer_path. Set AOS_TOKENIZER_PATH to a valid tokenizer.json."
fi

if [ "$backend" = "coreml" ] && [ "$(uname -s)" != "Darwin" ]; then
    die "CoreML backend requires macOS."
fi
if [ "$backend" = "coreml" ] && [[ "$model_path" != *.mlpackage ]]; then
    die "CoreML backend requires a .mlpackage model directory (got $model_path)."
fi

if [ "$backend" = "coreml" ]; then
    COREML_TARGET_DIR="${AOS_WORKER_COREML_TARGET_DIR:-$(aos_target_dir_for_flow worker)/coreml}"
    COREML_TARGET_DIR="$(abspath "$COREML_TARGET_DIR")"
    COREML_LEGACY_TARGET_DIR="$ROOT_DIR/target/coreml"

    resolve_coreml_worker_bin() {
        local candidate
        for candidate in \
            "$COREML_TARGET_DIR/debug/aos-worker" \
            "$COREML_LEGACY_TARGET_DIR/debug/aos-worker"; do
            if [ -x "$candidate" ]; then
                echo "$candidate"
                return 0
            fi
        done
        echo "$COREML_TARGET_DIR/debug/aos-worker"
        return 1
    }

    WORKER_BIN="$(resolve_coreml_worker_bin || true)"
    if [ ! -x "$WORKER_BIN" ]; then
        require_cmd cargo "Install Rust toolchain (rustup)."
        mkdir -p "$COREML_TARGET_DIR"
        info "Build CoreML-only worker binary"
        print_cmd cargo build -p adapteros-lora-worker --no-default-features --features multi-backend,coreml-backend --target-dir "$COREML_TARGET_DIR"
        if aos_is_truthy "${AOS_BUILD_USE_SCCACHE:-1}"; then
            cargo build -p adapteros-lora-worker --no-default-features --features multi-backend,coreml-backend --target-dir "$COREML_TARGET_DIR"
        else
            # Explicitly disable wrapper for troubleshooting parity with script-level policy.
            RUSTC_WRAPPER= cargo build -p adapteros-lora-worker --no-default-features --features multi-backend,coreml-backend --target-dir "$COREML_TARGET_DIR"
        fi
        WORKER_BIN="$(resolve_coreml_worker_bin || true)"
    fi
else
    WORKER_BIN="$(aos_resolve_binary aos-worker debug worker || true)"
    if [ ! -x "$WORKER_BIN" ]; then
        die "Worker binary not found: $WORKER_BIN. Build with: cargo build -p adapteros-lora-worker --features mlx"
    fi
    if [ "$backend" = "mlx" ]; then
        if ! "$WORKER_BIN" --help 2>&1 | grep -qi "mlx"; then
            die "Backend 'mlx' requested but worker binary likely built without MLX features. Rebuild with: cargo build -p adapteros-lora-worker --features mlx"
        fi
    fi
fi

mkdir -p "$ROOT_DIR/var/run" "$ROOT_DIR/var/logs" "$ROOT_DIR/var/tmp"

LOG_FILE="$ROOT_DIR/var/logs/dev-worker.log"
PID_FILE="$ROOT_DIR/var/run/dev-worker.pid"

BASE_URL="http://127.0.0.1:$(read_server_port "$CONFIG_PATH")"
export AOS_CP_URL="${AOS_CP_URL:-$BASE_URL}"
print_kv "cp_url" "$AOS_CP_URL"

if [ -f "$PID_FILE" ]; then
    existing_pid="$(cat "$PID_FILE" 2>/dev/null || true)"
    if [ -n "${existing_pid:-}" ] && kill -0 "$existing_pid" 2>/dev/null; then
        info "Worker already running"
        print_kv "pid" "$existing_pid"
        existing_cmd="$(ps -p "$existing_pid" -o command= 2>/dev/null || true)"
        if [ -n "$existing_cmd" ]; then
            echo "worker_cmd: $existing_cmd"
        fi
        worker_already_running=1
    else
        rm -f "$PID_FILE"
    fi
fi

if [ -z "${worker_already_running:-}" ] && [ -S "$uds_path" ]; then
    socket_pid="$(lsof -t "$uds_path" 2>/dev/null | head -1 || true)"
    if [ -n "$socket_pid" ] && kill -0 "$socket_pid" 2>/dev/null; then
        echo "$socket_pid" > "$PID_FILE"
        info "Worker already running (socket in use)"
        print_kv "pid" "$socket_pid"
        existing_cmd="$(ps -p "$socket_pid" -o command= 2>/dev/null || true)"
        if [ -n "$existing_cmd" ]; then
            echo "worker_cmd: $existing_cmd"
        fi
        worker_already_running=1
    else
        echo "Removing stale worker socket: $uds_path"
        rm -f "$uds_path"
    fi
fi

if [ -z "${worker_already_running:-}" ]; then
    if [ -n "$manifest_hash" ]; then
        export AOS_MANIFEST_HASH="$manifest_hash"
    fi
    export RUST_LOG="${RUST_LOG:-info,adapteros_lora_worker=info}"

    worker_cmd=(
        "$WORKER_BIN"
        --manifest "$manifest_path"
        --model-path "$model_path"
        --uds-path "$uds_path"
        --backend "$backend"
    )
    if [ -n "$manifest_hash" ]; then
        worker_cmd+=(--manifest-hash "$manifest_hash")
    fi

    if [ -n "$tokenizer_path" ] && [ -f "$tokenizer_path" ]; then
        worker_cmd+=(--tokenizer "$tokenizer_path")
    fi

    WORKER_START_EPOCH="$(date -u +%s)"
    info "Start worker"
    print_cmd "${worker_cmd[@]}"
    nohup "${worker_cmd[@]}" >"$LOG_FILE" 2>&1 &
    worker_pid=$!
    echo "$worker_pid" > "$PID_FILE"
    print_kv "pid" "$worker_pid"
fi

print_kv "pid_file" "$PID_FILE"
print_kv "log_file" "$LOG_FILE"

log_tail() {
    if [ ! -f "$LOG_FILE" ]; then
        return 0
    fi
    tail -n 200 "$LOG_FILE" | sed -E 's/\x1b\\[[0-9;]*m//g'
}

extract_worker_id() {
    local line
    line="$(log_tail | grep -E 'worker_id=' | tail -n 1 || true)"
    if [ -z "$line" ]; then
        return 1
    fi
    echo "$line" | sed -n 's/.*worker_id=\\([^ ,}]*\\).*/\\1/p'
}

coreml_ready_in_log() {
    if [ "$backend" != "coreml" ]; then
        return 0
    fi
    if [ "${worker_already_running:-}" = "1" ]; then
        if [ -z "${COREML_GATE_NOTE_PRINTED:-}" ]; then
            echo "NOTE: existing worker already running; skipping CoreML readiness gate."
            COREML_GATE_NOTE_PRINTED=1
        fi
        return 0
    fi
    local recent
    recent="$(log_tail || true)"
    if echo "$recent" | grep -q "Resolved backend selection" && echo "$recent" | grep -q "selected=coreml"; then
        return 0
    fi
    if echo "$recent" | grep -q "Initialized CoreML backend"; then
        return 0
    fi
    return 1
}

dev_no_auth_ready() {
    if ! is_truthy "${AOS_DEV_NO_AUTH:-}"; then
        return 1
    fi
    if [ -S "$uds_path" ]; then
        return 0
    fi
    local recent
    recent="$(log_tail || true)"
    if echo "$recent" | grep -q "UDS server listening"; then
        return 0
    fi
    return 1
}

check_coreml_fallback() {
    if [ "$backend" != "coreml" ]; then
        return 0
    fi
    if [ ! -f "$LOG_FILE" ]; then
        return 0
    fi
    local recent
    recent="$(log_tail || true)"
    if echo "$recent" | grep -q "coreml_unavailable_fallback"; then
        echo "CoreML backend was overridden. Last 40 lines of $LOG_FILE:"
        tail -n 40 "$LOG_FILE"
        die "CoreML backend unavailable; worker fell back. Ensure ANE/CoreML support or rebuild with: cargo build -p adapteros-lora-worker --no-default-features --features multi-backend,coreml-backend"
    fi
    if echo "$recent" | grep -q "Requested CoreML backend is not available"; then
        echo "CoreML backend not available. Last 40 lines of $LOG_FILE:"
        tail -n 40 "$LOG_FILE"
        die "CoreML backend unavailable on this host. Verify Metal/ANE access and CoreML support."
    fi
}

verify_workers_endpoint() {
    local url="$1"
    local must_contain="${2:-}"
    local must_contain_lower=""
    if [ -n "$must_contain" ]; then
        must_contain_lower="$(lower "$must_contain")"
    fi
    local tmp
    tmp="$(mktemp "$ROOT_DIR/var/tmp/worker-up.curl.XXXXXX")"
    local curl_exit=0
    curl -sS -i --max-time 5 "$url" >"$tmp" || curl_exit=$?
    if [ "$curl_exit" -ne 0 ]; then
        rm -f "$tmp"
        return 2
    fi
    local status
    status="$(awk 'NR==1 {print $2}' "$tmp")"
    local location
    location="$(awk 'tolower($1) == "location:" {print $2}' "$tmp" | tr -d '\r')"
    if [ "$status" = "401" ] || [ "$status" = "403" ]; then
        rm -f "$tmp"
        AUTH_BLOCKED=1
        return 3
    fi
    case "$status" in
        302|303|307|308)
            if echo "$location" | grep -qi "login"; then
                rm -f "$tmp"
                AUTH_BLOCKED=1
                return 3
            fi
            ;;
    esac
    if [ "$status" != "200" ]; then
        rm -f "$tmp"
        return 1
    fi
    local body
    body="$(awk 'BEGIN{body=0} /^\r?$/{body=1; next} {if(body) print}' "$tmp")"
    local compact
    compact="$(echo "$body" | tr -d '[:space:]')"
    if [ -n "$compact" ] && [ "$compact" != "[]" ]; then
        if [ -n "$must_contain_lower" ]; then
            local body_lower
            body_lower="$(echo "$body" | tr '[:upper:]' '[:lower:]')"
            if ! echo "$body_lower" | grep -Fq "$must_contain_lower"; then
                rm -f "$tmp"
                return 1
            fi
        fi
        echo "+ curl -i $url | head -30"
        head -n 30 "$tmp"
        rm -f "$tmp"
        return 0
    fi
    rm -f "$tmp"
    return 1
}

verify_registration_log() {
    local needle="Worker registration successful"
    local start_epoch="${WORKER_START_EPOCH:-}"
    local log
    for log in "$LOG_FILE" "$ROOT_DIR/var/logs/dev-up-backend.log" "$ROOT_DIR/var/logs/backend.log"; do
        if [ ! -f "$log" ]; then
            continue
        fi
        local line
        line="$(grep -n "$needle" "$log" | tail -n 1)"
        if [ -z "$line" ]; then
            continue
        fi
        if [ -n "$start_epoch" ]; then
            local line_json ts ts_epoch
            line_json="${line#*:}"
            ts="$(echo "$line_json" | sed -n 's/.*"ts":"\\([^"]*\\)".*/\\1/p')"
            if [ -z "$ts" ]; then
                continue
            fi
            ts_epoch="$(date -u -j -f "%Y-%m-%dT%H:%M:%SZ" "$ts" +%s 2>/dev/null || true)"
            if [ -z "$ts_epoch" ] || [ "$ts_epoch" -lt "$start_epoch" ]; then
                continue
            fi
        fi
        echo "+ grep -n \"$needle\" \"$log\" | tail -n 1"
        echo "$line"
        return 0
    done
    return 1
}

info "Verify readiness"
workers_url="$BASE_URL/v1/workers"
waited=0
timeout="${AOS_WORKER_REGISTER_TIMEOUT:-30}"
while [ "$waited" -lt "$timeout" ]; do
    check_coreml_fallback
    if [ "$backend" = "coreml" ] && ! coreml_ready_in_log; then
        sleep 1
        waited=$((waited + 1))
        continue
    fi
    if [ -f "$PID_FILE" ]; then
        current_pid="$(cat "$PID_FILE" 2>/dev/null || true)"
        if [ -n "${current_pid:-}" ] && ! kill -0 "$current_pid" 2>/dev/null; then
            if [ -f "$LOG_FILE" ]; then
                echo "Worker process exited (pid $current_pid). Last 40 lines of $LOG_FILE:"
                tail -n 40 "$LOG_FILE"
                if grep -q "multi-backend" "$LOG_FILE"; then
                    die "Worker exited: MLX backend requires the 'mlx' feature. Rebuild with: cargo build -p adapteros-lora-worker --features mlx"
                fi
            fi
            die "Worker process exited. Check $LOG_FILE."
        fi
    fi
    if dev_no_auth_ready; then
        echo "PASS: worker ready (dev no-auth)"
        echo "Next: hydration evidence + adapter load"
        exit 0
    fi
    endpoint_result=1
    worker_id="$(extract_worker_id || true)"
    if [ "$backend" = "coreml" ] && [ -z "${worker_id:-}" ] && [ "${worker_already_running:-}" != "1" ]; then
        sleep 1
        waited=$((waited + 1))
        continue
    fi
    if [ -n "${worker_id:-}" ]; then
        match_token="$worker_id"
    else
        match_token="$uds_path"
    fi
    if verify_workers_endpoint "$workers_url" "$match_token"; then
        endpoint_result=0
    else
        endpoint_result=$?
    fi
    if [ "$endpoint_result" -eq 0 ]; then
        echo "PASS: worker registered"
        echo "Next: hydration evidence + adapter load"
        exit 0
    fi
    if verify_registration_log; then
        echo "PASS: worker registered"
        echo "Next: hydration evidence + adapter load"
        exit 0
    fi
    sleep 1
    waited=$((waited + 1))
    if [ "${AUTH_BLOCKED:-0}" -eq 1 ]; then
        break
    fi
    done

if [ "${AUTH_BLOCKED:-0}" -eq 1 ]; then
    die "Auth blocked /v1/workers; provide credentials or use a dev-no-auth server to verify registration."
fi

die "Worker registration not confirmed after ${timeout}s. Check $LOG_FILE and backend logs."
