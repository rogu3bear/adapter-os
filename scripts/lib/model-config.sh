#!/usr/bin/env bash
# Shared model runtime configuration resolver for shell entrypoints.
# Keeps model path + base id + manifest resolution consistent across scripts.

aos_expand_path() {
    local path="${1:-}"
    local project_root="${2:-}"
    if [ -z "$path" ]; then
        return 0
    fi
    path="${path/#\~/$HOME}"
    if [[ "$path" != /* ]]; then
        path="${project_root%/}/${path#./}"
    fi
    path="$(printf '%s' "$path" | sed "s#/\\./#/#g")"
    printf '%s\n' "$path"
}

aos_manifest_slug() {
    local raw="${1:-}"
    local slug
    slug="$(printf '%s' "$raw" \
        | tr '[:upper:]' '[:lower:]' \
        | sed -E 's/[[:space:]_]+/-/g; s/[^a-z0-9.-]+/-/g; s/\.//g; s/-+/-/g; s/^-+//; s/-+$//')"
    printf '%s\n' "$slug"
}

aos_guess_manifest_path() {
    local project_root="${1:-}"
    local model_id="${2:-}"
    local model_path="${3:-}"
    local base_name=""
    if [ -n "$model_path" ]; then
        base_name="$(basename "$model_path")"
    fi

    local candidates=()
    local source=""
    for source in "$model_id" "$base_name"; do
        [ -n "$source" ] || continue
        local slug
        slug="$(aos_manifest_slug "$source")"
        [ -n "$slug" ] || continue
        candidates+=("$project_root/manifests/${slug}-base-only.yaml")
        candidates+=("$project_root/manifests/${slug}-mlx-base-only.yaml")
        candidates+=("$project_root/manifests/${slug}.yaml")

        local qwen_family=""
        if [[ "$slug" =~ (qwen35-[0-9]+b) ]]; then
            qwen_family="${BASH_REMATCH[1]}"
            candidates+=("$project_root/manifests/${qwen_family}-mlx-base-only.yaml")
            candidates+=("$project_root/manifests/${qwen_family}-base-only.yaml")
            candidates+=("$project_root/manifests/${qwen_family}.yaml")
        fi
    done

    candidates+=("$project_root/manifests/qwen35-27b-mlx-base-only.yaml")

    local candidate=""
    for candidate in "${candidates[@]}"; do
        if [ -f "$candidate" ]; then
            printf '%s\n' "$candidate"
            return 0
        fi
    done
    return 1
}

aos_read_runtime_model_settings() {
    local project_root="${1:-}"
    local runtime_config_path="${AOS_RUNTIME_CONFIG_PATH:-$project_root/var/config/runtime_config.v1.json}"

    [ -f "$runtime_config_path" ] || return 0

    python3 - "$runtime_config_path" <<'PY'
import json
import pathlib
import sys

runtime_config_path = pathlib.Path(sys.argv[1])
try:
    payload = json.loads(runtime_config_path.read_text(encoding="utf-8"))
except Exception:
    raise SystemExit(0)

if not isinstance(payload, dict):
    raise SystemExit(0)

settings = payload.get("settings")
if not isinstance(settings, dict):
    raise SystemExit(0)

models = settings.get("models")
if not isinstance(models, dict):
    raise SystemExit(0)

for key in ("selected_model_path", "selected_manifest_path"):
    value = models.get(key)
    if isinstance(value, str):
        value = value.strip()
        if value:
            print(f"{key}={value}")
PY
}

aos_resolve_model_runtime_env() {
    local project_root="${1:-}"
    if [ -z "$project_root" ]; then
        echo "aos_resolve_model_runtime_env requires project root argument" >&2
        return 1
    fi

    local default_cache_root="${AOS_DEFAULT_MODEL_CACHE_DIR:-var/models}"
    local default_model_id="${AOS_DEFAULT_BASE_MODEL_ID:-Qwen3.5-27B}"

    local model_path="${AOS_MODEL_PATH:-}"
    local cache_root="${AOS_MODEL_CACHE_DIR:-}"
    local base_model_id="${AOS_BASE_MODEL_ID:-}"
    local runtime_selected_model_path=""
    local runtime_selected_manifest_path=""

    while IFS='=' read -r key value; do
        case "$key" in
            selected_model_path) runtime_selected_model_path="$value" ;;
            selected_manifest_path) runtime_selected_manifest_path="$value" ;;
        esac
    done < <(aos_read_runtime_model_settings "$project_root")

    if [ -n "$model_path" ]; then
        model_path="$(aos_expand_path "$model_path" "$project_root")"
    elif [ -n "$runtime_selected_model_path" ]; then
        model_path="$(aos_expand_path "$runtime_selected_model_path" "$project_root")"
    fi
    if [ -n "$cache_root" ]; then
        cache_root="$(aos_expand_path "$cache_root" "$project_root")"
    fi

    if [ -z "$model_path" ] && [ -n "$cache_root" ] && [ -n "$base_model_id" ]; then
        model_path="${cache_root%/}/${base_model_id}"
    fi

    if [ -z "$model_path" ]; then
        if [ -z "$cache_root" ]; then
            cache_root="$(aos_expand_path "$default_cache_root" "$project_root")"
        fi
        if [ -z "$base_model_id" ]; then
            base_model_id="$default_model_id"
        fi
        model_path="${cache_root%/}/${base_model_id}"
    fi

    if [ -z "$cache_root" ]; then
        cache_root="$(dirname "$model_path")"
    fi
    if [ -z "$base_model_id" ]; then
        base_model_id="$(basename "$model_path")"
    fi

    export AOS_MODEL_PATH="$model_path"
    export AOS_MODEL_CACHE_DIR="$cache_root"
    export AOS_BASE_MODEL_ID="$base_model_id"

    local manifest_path="${AOS_WORKER_MANIFEST:-${AOS_MANIFEST_PATH:-}}"
    if [ -n "$manifest_path" ]; then
        manifest_path="$(aos_expand_path "$manifest_path" "$project_root")"
    elif [ -n "$runtime_selected_manifest_path" ]; then
        manifest_path="$(aos_expand_path "$runtime_selected_manifest_path" "$project_root")"
    else
        manifest_path="$(aos_guess_manifest_path "$project_root" "$base_model_id" "$model_path" || true)"
    fi

    if [ -n "${manifest_path:-}" ]; then
        export AOS_WORKER_MANIFEST="$manifest_path"
        export AOS_MANIFEST_PATH="${AOS_MANIFEST_PATH:-$manifest_path}"
    fi
}
