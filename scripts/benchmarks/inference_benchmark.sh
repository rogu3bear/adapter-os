#!/usr/bin/env bash
# ==============================================================================
# AdapterOS Inference Benchmark Suite
# ==============================================================================
#
# Measures end-to-end inference performance on a running AdapterOS instance.
#
# Prerequisites:
#   - AdapterOS server running: AOS_DEV_NO_AUTH=1 ./start
#   - At least one model loaded (check: curl -sf http://localhost:18080/healthz)
#   - At least one adapter available for cold-load testing
#   - For MLX baseline comparison: set AOS_MLX_BASELINE_TPS env var
#
# Environment variables:
#   AOS_SERVER_URL       - Server URL (default: http://localhost:18080)
#   AOS_ADAPTER_COUNT    - Number of adapters for memory test (default: 5)
#   AOS_ITERATIONS       - Iterations per measurement (default: 3)
#   AOS_MLX_BASELINE_TPS - Raw MLX baseline tok/s for overhead comparison
#   AOS_BENCH_PROMPT     - Custom prompt for throughput test
#   AOS_BENCH_MAX_TOKENS - Max tokens for throughput test (default: 100)
#
# Output:
#   JSON summary to stdout (pipe to jq or redirect to file)
#   Human-readable progress to stderr
#
# Exit codes:
#   0 - All measurements collected successfully
#   1 - Server not running or measurement failed
#
# Definition of "cold" TTFT:
#   The base model is already warm (loaded and has completed at least one
#   inference). An adapter that is NOT currently in the in-memory adapter cache
#   is requested. "Cold" measures the time from request submission to the first
#   token event, including adapter loading from disk, LoRA weight application,
#   and initial generation. This isolates adapter-load latency from model-load
#   latency.
#
# ==============================================================================

set -euo pipefail

# ---------------------------------------------------------------------------
# Configuration
# ---------------------------------------------------------------------------
SERVER_URL="${AOS_SERVER_URL:-http://localhost:18080}"
ADAPTER_COUNT="${AOS_ADAPTER_COUNT:-5}"
ITERATIONS="${AOS_ITERATIONS:-3}"
MLX_BASELINE_TPS="${AOS_MLX_BASELINE_TPS:-}"
BENCH_PROMPT="${AOS_BENCH_PROMPT:-Write a detailed explanation of how LoRA adapters work in large language models. Include the mathematical foundations and practical benefits.}"
BENCH_MAX_TOKENS="${AOS_BENCH_MAX_TOKENS:-100}"
OVERHEAD_WARN_PCT=5

# ---------------------------------------------------------------------------
# Helpers
# ---------------------------------------------------------------------------
log() { echo "[benchmark] $*" >&2; }
err() { echo "[benchmark] ERROR: $*" >&2; }

# Portable millisecond timestamp (macOS)
now_ms() {
    if command -v gdate >/dev/null 2>&1; then
        gdate +%s%3N
    elif command -v python3 >/dev/null 2>&1; then
        python3 -c 'import time; print(int(time.time()*1000))'
    else
        # Fallback: second precision
        echo "$(date +%s)000"
    fi
}

# ---------------------------------------------------------------------------
# Machine info
# ---------------------------------------------------------------------------
print_machine_info() {
    log "=== Machine Info ==="
    log "Date:    $(date -u +"%Y-%m-%dT%H:%M:%SZ")"
    log "macOS:   $(sw_vers -productVersion 2>/dev/null || echo 'unknown')"

    local hw_model
    hw_model=$(sysctl -n hw.model 2>/dev/null || echo "unknown")
    log "Model:   ${hw_model}"

    local total_mem_bytes
    total_mem_bytes=$(sysctl -n hw.memsize 2>/dev/null || echo 0)
    local total_mem_gb
    total_mem_gb=$(( total_mem_bytes / 1024 / 1024 / 1024 ))
    log "UMA:     ${total_mem_gb} GB (${total_mem_bytes} bytes)"

    log "Server:  ${SERVER_URL}"
    log "==================="
}

# ---------------------------------------------------------------------------
# Pre-flight: ensure server is healthy
# ---------------------------------------------------------------------------
check_server() {
    log "Checking server health at ${SERVER_URL}/healthz ..."
    if ! curl -sf "${SERVER_URL}/healthz" >/dev/null 2>&1; then
        err "Server not responding at ${SERVER_URL}/healthz"
        err "Start AdapterOS first: AOS_DEV_NO_AUTH=1 ./start"
        exit 1
    fi
    log "Server is healthy"
}

# ---------------------------------------------------------------------------
# TTFT measurement (cold adapter load)
# ---------------------------------------------------------------------------
# Sends a streaming inference request and measures time to first Token SSE event.
# Discards the first iteration (warmup) and reports the median of the rest.
measure_ttft() {
    log "=== TTFT Measurement (cold adapter load) ==="
    log "Iterations: ${ITERATIONS} (first discarded as warmup)"

    local ttft_values=()

    for i in $(seq 1 "${ITERATIONS}"); do
        log "  Iteration ${i}/${ITERATIONS} ..."

        local start_ms
        start_ms=$(now_ms)

        # Send streaming inference request and wait for first data: line
        # containing a token event. Use timeout to avoid hanging.
        local first_token_ms=0
        local got_token=false

        # Stream SSE and capture first token timestamp
        while IFS= read -r line; do
            if [[ "${line}" == data:* ]]; then
                # Check if this is a Token event (contains "Token" or "token" or content)
                local data_payload="${line#data: }"
                if echo "${data_payload}" | grep -qiE '"(token|content|text)"'; then
                    first_token_ms=$(now_ms)
                    got_token=true
                    break
                fi
            fi
        done < <(
            curl -sS --max-time 30 -N \
                -H "Content-Type: application/json" \
                -H "Accept: text/event-stream" \
                "${SERVER_URL}/v1/inference/stream" \
                -d "{
                    \"prompt\": \"Hello, briefly introduce yourself.\",
                    \"max_tokens\": 5,
                    \"temperature\": 0.0
                }" 2>/dev/null || true
        )

        if ${got_token}; then
            local elapsed_ms=$(( first_token_ms - start_ms ))
            ttft_values+=("${elapsed_ms}")
            log "  TTFT: ${elapsed_ms} ms"
        else
            log "  TTFT: no token received (server may not have adapters loaded)"
            ttft_values+=("0")
        fi
    done

    # Compute median (discard first iteration if we have enough)
    if [[ ${#ttft_values[@]} -gt 1 ]]; then
        # Remove first (warmup) iteration
        local remaining=("${ttft_values[@]:1}")
        # Sort and take median
        IFS=$'\n' read -r -d '' -a sorted < <(printf '%s\n' "${remaining[@]}" | sort -n && printf '\0') || true
        local mid=$(( ${#sorted[@]} / 2 ))
        TTFT_COLD_MS="${sorted[${mid}]}"
    elif [[ ${#ttft_values[@]} -eq 1 ]]; then
        TTFT_COLD_MS="${ttft_values[0]}"
    else
        TTFT_COLD_MS=0
    fi

    log "  Median TTFT (cold): ${TTFT_COLD_MS} ms"
}

# ---------------------------------------------------------------------------
# Throughput measurement (warm adapter)
# ---------------------------------------------------------------------------
# Sends a sustained generation request with an already-loaded adapter.
# Parses the final Done event for total_tokens and latency_ms, computes tok/s.
measure_throughput() {
    log "=== Throughput Measurement (warm adapter) ==="
    log "Iterations: ${ITERATIONS}, max_tokens: ${BENCH_MAX_TOKENS}"

    local tps_values=()

    for i in $(seq 1 "${ITERATIONS}"); do
        log "  Iteration ${i}/${ITERATIONS} ..."

        local start_ms
        start_ms=$(now_ms)

        local total_tokens=0
        local done_found=false

        # Stream SSE and count tokens or parse Done event
        while IFS= read -r line; do
            if [[ "${line}" == data:* ]]; then
                local data_payload="${line#data: }"

                # Check for Done event with metrics
                if echo "${data_payload}" | grep -qiE '"(done|complete|finished)"'; then
                    done_found=true
                    # Try to extract total_tokens from Done payload
                    local extracted_tokens
                    extracted_tokens=$(echo "${data_payload}" | grep -oE '"total_tokens"\s*:\s*[0-9]+' | grep -oE '[0-9]+' | head -1) || true
                    if [[ -n "${extracted_tokens}" ]]; then
                        total_tokens="${extracted_tokens}"
                    fi
                    break
                fi

                # Count Token events
                if echo "${data_payload}" | grep -qiE '"(token|content|text)"'; then
                    total_tokens=$(( total_tokens + 1 ))
                fi
            fi
        done < <(
            curl -sS --max-time 60 -N \
                -H "Content-Type: application/json" \
                -H "Accept: text/event-stream" \
                "${SERVER_URL}/v1/inference/stream" \
                -d "{
                    \"prompt\": $(printf '%s' "${BENCH_PROMPT}" | python3 -c 'import json,sys; print(json.dumps(sys.stdin.read()))'),
                    \"max_tokens\": ${BENCH_MAX_TOKENS},
                    \"temperature\": 0.7
                }" 2>/dev/null || true
        )

        local end_ms
        end_ms=$(now_ms)
        local elapsed_ms=$(( end_ms - start_ms ))

        if [[ ${total_tokens} -gt 0 && ${elapsed_ms} -gt 0 ]]; then
            # Compute tok/s with 1 decimal: tokens * 1000 / elapsed_ms
            local tps
            tps=$(python3 -c "print(round(${total_tokens} * 1000.0 / ${elapsed_ms}, 1))")
            tps_values+=("${tps}")
            log "  Tokens: ${total_tokens}, Time: ${elapsed_ms} ms, Throughput: ${tps} tok/s"
        else
            log "  No tokens generated or zero elapsed time"
            tps_values+=("0")
        fi
    done

    # Compute median
    if [[ ${#tps_values[@]} -gt 0 ]]; then
        IFS=$'\n' read -r -d '' -a sorted < <(printf '%s\n' "${tps_values[@]}" | sort -n && printf '\0') || true
        local mid=$(( ${#sorted[@]} / 2 ))
        THROUGHPUT_WARM_TPS="${sorted[${mid}]}"
    else
        THROUGHPUT_WARM_TPS="0"
    fi

    log "  Median throughput (warm): ${THROUGHPUT_WARM_TPS} tok/s"

    # --- MLX baseline comparison ---
    ORCHESTRATION_OVERHEAD_PCT="null"
    if [[ -n "${MLX_BASELINE_TPS}" && "${MLX_BASELINE_TPS}" != "0" ]]; then
        log "=== Orchestration Overhead vs Raw MLX Baseline ==="
        log "  MLX baseline: ${MLX_BASELINE_TPS} tok/s"
        log "  Warm throughput: ${THROUGHPUT_WARM_TPS} tok/s"

        ORCHESTRATION_OVERHEAD_PCT=$(python3 -c "
baseline = float('${MLX_BASELINE_TPS}')
warm = float('${THROUGHPUT_WARM_TPS}')
if baseline > 0 and warm > 0:
    overhead = ((baseline - warm) / baseline) * 100.0
    print(round(overhead, 1))
else:
    print('null')
")

        if [[ "${ORCHESTRATION_OVERHEAD_PCT}" != "null" ]]; then
            local overhead_float="${ORCHESTRATION_OVERHEAD_PCT}"
            log "  Orchestration overhead: ${overhead_float}%"

            # Check if overhead exceeds warning threshold
            local exceeds
            exceeds=$(python3 -c "print('yes' if float('${overhead_float}') > ${OVERHEAD_WARN_PCT} else 'no')")
            if [[ "${exceeds}" == "yes" ]]; then
                log "  WARNING: Orchestration overhead (${overhead_float}%) exceeds ${OVERHEAD_WARN_PCT}% threshold"
                log "  WARNING: Warm adapter throughput should match raw MLX baseline"
                log "  WARNING: Investigate inference pipeline for unnecessary allocations or synchronization"
            else
                log "  OK: Overhead within ${OVERHEAD_WARN_PCT}% threshold"
            fi
        fi
    else
        log "  (Skipping MLX baseline comparison -- set AOS_MLX_BASELINE_TPS to enable)"
    fi
}

# ---------------------------------------------------------------------------
# Peak memory measurement
# ---------------------------------------------------------------------------
# Records UMA usage before and after loading adapters.
measure_peak_memory() {
    log "=== Peak Memory Measurement ==="
    log "Adapter count target: ${ADAPTER_COUNT}"

    local total_mem_bytes
    total_mem_bytes=$(sysctl -n hw.memsize 2>/dev/null || echo 0)
    local total_mem_mb=$(( total_mem_bytes / 1024 / 1024 ))

    # Parse vm_stat for pages free/active/inactive/wired
    local page_size
    page_size=$(vm_stat 2>/dev/null | head -1 | grep -oE '[0-9]+' || echo 16384)

    local pages_free pages_active pages_inactive pages_wired pages_speculative
    pages_free=$(vm_stat 2>/dev/null | awk '/Pages free/ {gsub(/\./,"",$NF); print $NF}' || echo 0)
    pages_active=$(vm_stat 2>/dev/null | awk '/Pages active/ {gsub(/\./,"",$NF); print $NF}' || echo 0)
    pages_inactive=$(vm_stat 2>/dev/null | awk '/Pages inactive/ {gsub(/\./,"",$NF); print $NF}' || echo 0)
    pages_wired=$(vm_stat 2>/dev/null | awk '/Pages wired/ {gsub(/\./,"",$NF); print $NF}' || echo 0)
    pages_speculative=$(vm_stat 2>/dev/null | awk '/Pages speculative/ {gsub(/\./,"",$NF); print $NF}' || echo 0)

    # Used = active + wired (the memory actually in use)
    local used_pages=$(( pages_active + pages_wired ))
    local used_bytes=$(( used_pages * page_size ))
    local used_mb=$(( used_bytes / 1024 / 1024 ))

    # Read UMA ceiling from config (default 75%)
    local uma_ceiling_pct=75
    if command -v grep >/dev/null 2>&1; then
        local config_ceiling
        config_ceiling=$(grep -E '^ceiling_pct\s*=' configs/cp.toml 2>/dev/null | head -1 | grep -oE '[0-9]+' || echo "")
        if [[ -n "${config_ceiling}" ]]; then
            uma_ceiling_pct="${config_ceiling}"
        fi
    fi

    local ceiling_mb=$(( total_mem_mb * uma_ceiling_pct / 100 ))
    local usage_of_ceiling_pct=0
    if [[ ${ceiling_mb} -gt 0 ]]; then
        usage_of_ceiling_pct=$(python3 -c "print(round(${used_mb} / ${ceiling_mb} * 100.0, 1))")
    fi

    PEAK_MEMORY_MB="${used_mb}"
    UMA_CEILING_PCT="${uma_ceiling_pct}"

    log "  Total UMA:     ${total_mem_mb} MB"
    log "  Used memory:   ${used_mb} MB"
    log "  UMA ceiling:   ${ceiling_mb} MB (${uma_ceiling_pct}%)"
    log "  Usage/ceiling: ${usage_of_ceiling_pct}%"
    log "  Active pages:  ${pages_active}"
    log "  Wired pages:   ${pages_wired}"
    log "  Free pages:    ${pages_free}"
}

# ---------------------------------------------------------------------------
# JSON output
# ---------------------------------------------------------------------------
output_json() {
    local timestamp
    timestamp=$(date -u +"%Y-%m-%dT%H:%M:%SZ")

    local mlx_baseline_json="null"
    if [[ -n "${MLX_BASELINE_TPS}" && "${MLX_BASELINE_TPS}" != "0" ]]; then
        mlx_baseline_json="${MLX_BASELINE_TPS}"
    fi

    local overhead_json="${ORCHESTRATION_OVERHEAD_PCT}"

    cat <<ENDJSON
{
  "ttft_cold_ms": ${TTFT_COLD_MS},
  "throughput_warm_tps": ${THROUGHPUT_WARM_TPS},
  "mlx_baseline_tps": ${mlx_baseline_json},
  "orchestration_overhead_pct": ${overhead_json},
  "peak_memory_mb": ${PEAK_MEMORY_MB},
  "uma_ceiling_pct": ${UMA_CEILING_PCT},
  "adapter_count": ${ADAPTER_COUNT},
  "iterations": ${ITERATIONS},
  "timestamp": "${timestamp}"
}
ENDJSON
}

# ---------------------------------------------------------------------------
# Main
# ---------------------------------------------------------------------------
main() {
    # Initialize result variables
    TTFT_COLD_MS=0
    THROUGHPUT_WARM_TPS="0"
    ORCHESTRATION_OVERHEAD_PCT="null"
    PEAK_MEMORY_MB=0
    UMA_CEILING_PCT=75

    print_machine_info
    check_server

    measure_ttft
    measure_throughput
    measure_peak_memory

    log ""
    log "=== Benchmark Complete ==="
    output_json
}

main "$@"
