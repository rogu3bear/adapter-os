#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
ROOT_DIR="$(cd "$SCRIPT_DIR/.." && pwd)"
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

info "Preflight"
if [ ! -f "$ROOT_DIR/Cargo.toml" ] || [ ! -d "$ROOT_DIR/crates/adapteros-server" ]; then
    die "Run from the adapter-os repo root (missing Cargo.toml or crates/adapteros-server)."
fi

require_cmd rustc "Install Rust from https://rustup.rs/"
require_cmd cargo "Install Rust from https://rustup.rs/"
require_cmd curl "Install curl (macOS: brew install curl)."

print_kv "repo_root" "$ROOT_DIR"
print_kv "rustc" "$(rustc --version)"
print_kv "cargo" "$(cargo --version)"

print_env_if_set "AOS_DEV_NO_AUTH"
print_env_if_set "AOS_REFERENCE_MODE"
print_env_if_set "AOS_CONFIG"
print_env_if_set "AOS_MODEL_CACHE_DIR"
print_env_if_set "AOS_BASE_MODEL_ID"
print_env_if_set "AOS_MODEL_PATH"
print_env_if_set "AOS_TOKENIZER_PATH"

mkdir -p "$ROOT_DIR/var/tmp"

CONFIG_PATH="${AOS_CONFIG:-$ROOT_DIR/configs/cp.toml}"
if [ ! -f "$CONFIG_PATH" ]; then
    die "Config not found: $CONFIG_PATH"
fi
print_kv "config_path" "$CONFIG_PATH"

STATIC_DIR="$ROOT_DIR/crates/adapteros-server/static"
UI_BUILD_CMD="$ROOT_DIR/scripts/ci/build_leptos_wasm.sh"

need_ui_build=0
if [ ! -d "$STATIC_DIR" ]; then
    need_ui_build=1
else
    wasm_count="$(ls "$STATIC_DIR"/*.wasm 2>/dev/null | wc -l | tr -d ' ')"
    css_count="$(ls "$STATIC_DIR"/*.css 2>/dev/null | wc -l | tr -d ' ')"
    js_count="$(ls "$STATIC_DIR"/*.js 2>/dev/null | wc -l | tr -d ' ')"
    if [ ! -f "$STATIC_DIR/index.html" ] || [ "$wasm_count" -eq 0 ] || { [ "$css_count" -eq 0 ] && [ "$js_count" -eq 0 ]; }; then
        need_ui_build=1
    fi
fi

if [ "$need_ui_build" -eq 1 ]; then
    info "Build UI assets"
    echo "+ $UI_BUILD_CMD"
    if [ ! -f "$UI_BUILD_CMD" ]; then
        die "UI build script not found: $UI_BUILD_CMD"
    fi
    bash "$UI_BUILD_CMD"
fi

info "Validate UI assets"
if [ ! -d "$STATIC_DIR" ]; then
    die "UI static directory missing: $STATIC_DIR (build command: $UI_BUILD_CMD)"
fi
if [ ! -f "$STATIC_DIR/index.html" ]; then
    die "UI index missing: $STATIC_DIR/index.html (build command: $UI_BUILD_CMD)"
fi
wasm_count="$(ls "$STATIC_DIR"/*.wasm 2>/dev/null | wc -l | tr -d ' ')"
css_count="$(ls "$STATIC_DIR"/*.css 2>/dev/null | wc -l | tr -d ' ')"
js_count="$(ls "$STATIC_DIR"/*.js 2>/dev/null | wc -l | tr -d ' ')"
if [ "$wasm_count" -eq 0 ]; then
    die "No .wasm assets found in $STATIC_DIR (build command: $UI_BUILD_CMD)"
fi
if [ "$css_count" -eq 0 ] && [ "$js_count" -eq 0 ]; then
    die "No .css or .js assets found in $STATIC_DIR (build command: $UI_BUILD_CMD)"
fi
if [ "$css_count" -eq 0 ] && [ "$js_count" -gt 0 ]; then
    echo "NOTE: No CSS assets found; using JS assets as equivalent."
fi
print_kv "ui_assets" "index.html wasm=$wasm_count css=$css_count js=$js_count"

info "Build backend (debug)"
echo "+ cargo build -p adapteros-server"
cargo build -p adapteros-server

info "Migrations"
CHECK_LOG="$(mktemp "$ROOT_DIR/var/tmp/dev-up.migrations.XXXXXX")"
if ! "$ROOT_DIR/scripts/check-migrations.sh" >"$CHECK_LOG" 2>&1; then
    if grep -q "Run: ./scripts/sign_migrations.sh" "$CHECK_LOG"; then
        echo "Migration signatures invalid; running scripts/sign_migrations.sh"
        echo "+ $ROOT_DIR/scripts/sign_migrations.sh"
        "$ROOT_DIR/scripts/sign_migrations.sh"
        if ! "$ROOT_DIR/scripts/check-migrations.sh" >"$CHECK_LOG" 2>&1; then
            cat "$CHECK_LOG" >&2
            rm -f "$CHECK_LOG"
            die "Migration signature check failed after re-signing."
        fi
    else
        cat "$CHECK_LOG" >&2
        rm -f "$CHECK_LOG"
        die "Migration signature check failed."
    fi
fi
rm -f "$CHECK_LOG"

SERVER_BIN="$ROOT_DIR/target/debug/adapteros-server"
if [ ! -x "$SERVER_BIN" ]; then
    die "Backend binary not found: $SERVER_BIN"
fi

echo "+ $SERVER_BIN --config \"$CONFIG_PATH\" --migrate-only"
"$SERVER_BIN" --config "$CONFIG_PATH" --migrate-only

info "Start backend"
mkdir -p "$ROOT_DIR/var/run" "$ROOT_DIR/var/logs" "$ROOT_DIR/var/tmp"
PORT="$(read_server_port "$CONFIG_PATH")"
BASE_URL="http://127.0.0.1:${PORT}"
PID_FILE="$ROOT_DIR/var/run/dev-up-backend.pid"
LOG_FILE="$ROOT_DIR/var/logs/dev-up-backend.log"

if [ -f "$PID_FILE" ]; then
    existing_pid="$(cat "$PID_FILE" 2>/dev/null || true)"
    if [ -n "${existing_pid:-}" ] && kill -0 "$existing_pid" 2>/dev/null; then
        echo "Backend already running (pid $existing_pid)."
    else
        rm -f "$PID_FILE"
    fi
fi

if [ ! -f "$PID_FILE" ]; then
    if curl -sS --max-time 2 "$BASE_URL/healthz" >/dev/null 2>&1; then
        echo "Backend already responding at $BASE_URL (skipping start)."
    else
        echo "+ nohup \"$SERVER_BIN\" --config \"$CONFIG_PATH\" > \"$LOG_FILE\" 2>&1 &"
        nohup "$SERVER_BIN" --config "$CONFIG_PATH" > "$LOG_FILE" 2>&1 &
        backend_pid=$!
        echo "$backend_pid" > "$PID_FILE"
    fi
fi

info "Backend checks"
health_waited=0
health_timeout="${AOS_HEALTH_TIMEOUT:-30}"
while [ "$health_waited" -lt "$health_timeout" ]; do
    if curl -sS --max-time 2 "$BASE_URL/healthz" >/dev/null 2>&1; then
        break
    fi
    if [ -f "$PID_FILE" ]; then
        running_pid="$(cat "$PID_FILE" 2>/dev/null || true)"
        if [ -n "${running_pid:-}" ] && ! kill -0 "$running_pid" 2>/dev/null; then
            echo "Backend process exited (pid $running_pid)."
            if [ -f "$LOG_FILE" ]; then
                echo "Last 30 lines of $LOG_FILE:"
                tail -n 30 "$LOG_FILE"
            fi
            die "Backend failed to start."
        fi
    fi
    sleep 1
    health_waited=$((health_waited + 1))
done
if ! curl -sS --max-time 2 "$BASE_URL/healthz" >/dev/null 2>&1; then
    if [ -f "$LOG_FILE" ]; then
        echo "Last 30 lines of $LOG_FILE:"
        tail -n 30 "$LOG_FILE"
    fi
    die "Backend did not become healthy within ${health_timeout}s."
fi

curl_evidence() {
    local url="$1"
    local lines="$2"
    local tmp
    tmp="$(mktemp "$ROOT_DIR/var/tmp/dev-up.curl.XXXXXX")"
    local curl_exit=0
    curl -sS -i --max-time 5 "$url" >"$tmp" || curl_exit=$?
    if [ "$curl_exit" -ne 0 ]; then
        rm -f "$tmp"
        die "Failed to reach $url (connection error)."
    fi
    echo "+ curl -i $url | head -$lines"
    head -n "$lines" "$tmp"
    echo ""

    local status
    status="$(awk 'NR==1 {print $2}' "$tmp")"
    LAST_STATUS="$status"
    if [ "$status" = "401" ] || [ "$status" = "403" ]; then
        rm -f "$tmp"
        echo "Auth blocked at $url (HTTP $status)."
        echo "Fix: export AOS_DEV_NO_AUTH=1 and rerun ./scripts/dev-up.sh"
        exit 1
    fi
    if [ "$status" = "302" ] || [ "$status" = "303" ] || [ "$status" = "307" ] || [ "$status" = "308" ]; then
        if grep -qi "^location: .*login" "$tmp"; then
            rm -f "$tmp"
            echo "Auth redirect at $url (HTTP $status)."
            echo "Fix: export AOS_DEV_NO_AUTH=1 and rerun ./scripts/dev-up.sh"
            exit 1
        fi
    fi
    rm -f "$tmp"
}

curl_evidence "$BASE_URL/healthz" 30
curl_evidence "$BASE_URL/readyz" 30
if [ "${LAST_STATUS:-}" != "200" ] && [ -n "${CI:-}" ]; then
    echo "CI note: /readyz is not 200; worker may be unavailable in CI."
fi
curl_evidence "$BASE_URL/" 80

info "Success"
echo "Open: $BASE_URL"
echo "Expected UI state: dashboard renders and chat empty state is visible."
