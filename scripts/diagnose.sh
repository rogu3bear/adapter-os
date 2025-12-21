#!/usr/bin/env bash
# AdapterOS Comprehensive Diagnostic Tool
# Generates detailed system health report for troubleshooting

set -euo pipefail

# Colors for output
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
BLUE='\033[0;34m'
NC='\033[0m' # No Color

# Configuration
REPORT_FILE="${1:-diagnostic_report_$(date +%Y%m%d_%H%M%S).txt}"
API_BASE="${AOS_API_BASE:-http://localhost:8080}"
VERBOSE="${VERBOSE:-0}"

# Helper functions
info() {
    echo -e "${BLUE}[INFO]${NC} $*"
}

success() {
    echo -e "${GREEN}[OK]${NC} $*"
}

warn() {
    echo -e "${YELLOW}[WARN]${NC} $*"
}

error() {
    echo -e "${RED}[ERROR]${NC} $*"
}

separator() {
    echo "========================================"
}

check_command() {
    if command -v "$1" &>/dev/null; then
        return 0
    else
        return 1
    fi
}

# Start report
{
    echo "==================================================================="
    echo "          AdapterOS Diagnostic Report"
    echo "==================================================================="
    echo "Generated: $(date)"
    echo "Hostname: $(hostname)"
    echo "User: $(whoami)"
    echo "Working Directory: $(pwd)"
    echo "==================================================================="
    echo

    # System Information
    separator
    echo "=== SYSTEM INFORMATION ==="
    separator

    if [[ "$OSTYPE" == "darwin"* ]]; then
        echo "Operating System: macOS"
        sw_vers
        echo
        echo "Hardware:"
        system_profiler SPHardwareDataType | grep -E "Model Name|Chip|Memory"
        echo
        echo "Metal Support:"
        system_profiler SPDisplaysDataType | grep -A 5 "Metal"
    else
        echo "Operating System: Linux"
        uname -a
        echo
        if check_command lsb_release; then
            lsb_release -a
        elif [ -f /etc/os-release ]; then
            cat /etc/os-release
        fi
        echo
        echo "Memory:"
        free -h
    fi
    echo

    # Rust Environment
    separator
    echo "=== RUST ENVIRONMENT ==="
    separator

    if check_command rustc; then
        rustc --version
        cargo --version
        if [ -f rust-toolchain.toml ]; then
            echo "Toolchain config:"
            cat rust-toolchain.toml
        fi
    else
        error "Rust not found in PATH"
    fi
    echo

    # Environment Variables
    separator
    echo "=== ENVIRONMENT VARIABLES ==="
    separator

    env | grep -E "^AOS_|^RUST_" | sort
    echo

    # Process Status
    separator
    echo "=== PROCESS STATUS ==="
    separator

    echo "AdapterOS Processes:"
    if ps aux | grep -E "adapteros-server|aos-worker" | grep -v grep; then
        success "Processes found"
    else
        error "No AdapterOS processes running"
    fi
    echo

    # Port Status
    separator
    echo "=== PORT STATUS ==="
    separator

    echo "Checking port 8080 (control plane):"
    if lsof -ti:8080 >/dev/null 2>&1; then
        lsof -i:8080
        success "Port 8080 in use"
    else
        error "Port 8080 not in use (service may not be running)"
    fi
    echo

    echo "Checking port 3200 (UI):"
    if lsof -ti:3200 >/dev/null 2>&1; then
        lsof -i:3200
        success "Port 3200 in use"
    else
        warn "Port 3200 not in use (UI may not be running)"
    fi
    echo

    # Health Checks
    separator
    echo "=== HEALTH CHECKS ==="
    separator

    echo "Testing /healthz endpoint:"
    if curl -f -s -m 5 "$API_BASE/healthz" >/dev/null 2>&1; then
        success "Health endpoint OK"
    else
        error "Health endpoint failed or unreachable"
    fi
    echo

    echo "Testing /readyz endpoint:"
    if readyz_response=$(curl -f -s -m 5 "$API_BASE/readyz" 2>/dev/null); then
        success "Ready endpoint OK"
        if check_command jq; then
            echo "$readyz_response" | jq .
        else
            echo "$readyz_response"
        fi
    else
        error "Ready endpoint failed or unreachable"
    fi
    echo

    # System Metrics
    separator
    echo "=== SYSTEM METRICS ==="
    separator

    if metrics=$(curl -f -s -m 5 "$API_BASE/api/v1/metrics/system" 2>/dev/null); then
        if check_command jq; then
            echo "Memory:"
            echo "$metrics" | jq '.memory'
            echo
            echo "Adapters:"
            echo "$metrics" | jq '.adapters'
            echo
            echo "Inference:"
            echo "$metrics" | jq '.inference'
            echo
            echo "Errors:"
            echo "$metrics" | jq '.errors'
        else
            echo "$metrics"
        fi
        success "Metrics retrieved successfully"
    else
        error "Failed to retrieve metrics from API"
    fi
    echo

    # Database Status
    separator
    echo "=== DATABASE STATUS ==="
    separator

    DB_PATH="var/aos-cp.sqlite3"

    if [ -f "$DB_PATH" ]; then
        success "Database file exists: $DB_PATH"
        ls -lh "$DB_PATH"
        echo

        if check_command sqlite3; then
            echo "Running integrity check:"
            if sqlite3 "$DB_PATH" "PRAGMA integrity_check;" 2>/dev/null; then
                success "Database integrity OK"
            else
                error "Database integrity check failed"
            fi
            echo

            echo "Database statistics:"
            echo "Tables:"
            sqlite3 "$DB_PATH" "SELECT name FROM sqlite_master WHERE type='table';" 2>/dev/null | wc -l
            echo
            echo "Adapters:"
            sqlite3 "$DB_PATH" "SELECT COUNT(*) FROM adapters;" 2>/dev/null
            echo
            echo "Tenants:"
            sqlite3 "$DB_PATH" "SELECT COUNT(*) FROM tenants;" 2>/dev/null
            echo
            echo "WAL file:"
            ls -lh "${DB_PATH}-wal" 2>/dev/null || echo "No WAL file"
            echo
        else
            warn "sqlite3 command not found, skipping detailed DB checks"
        fi
    else
        error "Database file not found at $DB_PATH"
    fi
    echo

    # Worker Status
    separator
    echo "=== WORKER STATUS ==="
    separator

    echo "Worker sockets:"
    if ls var/run/aos/*/worker.sock 2>/dev/null; then
        success "Worker socket files found"
        echo
        echo "Socket connections:"
        lsof var/run/aos/*/worker.sock 2>/dev/null || echo "No active connections"
    else
        error "No worker socket files found"
    fi
    echo

    # Disk Space
    separator
    echo "=== DISK SPACE ==="
    separator

    echo "Filesystem usage:"
    df -h . | tail -1
    echo
    echo "var/ directory usage:"
    du -sh var/ 2>/dev/null || echo "var/ directory not found"
    echo
    echo "Breakdown by subdirectory:"
    du -sh var/* 2>/dev/null | sort -hr | head -10
    echo

    # Log Analysis
    separator
    echo "=== LOG ANALYSIS ==="
    separator

    LOG_FILE="var/aos-cp.log"

    if [ -f "$LOG_FILE" ]; then
        success "Log file found: $LOG_FILE"
        echo "Log file size: $(ls -lh "$LOG_FILE" | awk '{print $5}')"
        echo

        echo "Recent errors (last 50):"
        grep ERROR "$LOG_FILE" | tail -50 || echo "No errors found"
        echo

        echo "Error summary (last 1000 lines):"
        tail -1000 "$LOG_FILE" | grep ERROR | \
            awk -F'ERROR' '{print $2}' | \
            cut -d':' -f1 | \
            sort | uniq -c | sort -nr | head -10 || echo "No errors found"
        echo

        echo "Recent warnings (last 20):"
        grep WARN "$LOG_FILE" | tail -20 || echo "No warnings found"
        echo
    else
        error "Log file not found at $LOG_FILE"
    fi

    # Worker Logs
    WORKER_LOG="var/logs/worker.log"
    if [ -f "$WORKER_LOG" ]; then
        echo "Worker log analysis:"
        echo "Recent worker errors:"
        grep -i "error\|fatal\|panic" "$WORKER_LOG" | tail -20 || echo "No worker errors found"
    else
        warn "Worker log not found at $WORKER_LOG"
    fi
    echo

    # Backend Detection
    separator
    echo "=== BACKEND CONFIGURATION ==="
    separator

    if [ -f "$LOG_FILE" ]; then
        echo "Detected backends:"
        grep -i "backend.*initialized" "$LOG_FILE" | tail -5
        echo
        echo "Stub warnings:"
        grep -i "stub.*active" "$LOG_FILE" | tail -5 || echo "No stub warnings (good)"
        echo
    fi

    # Configuration Files
    separator
    echo "=== CONFIGURATION FILES ==="
    separator

    if [ -f configs/aos.toml ]; then
        echo "Main config (configs/aos.toml) - redacted sensitive values:"
        grep -v -E "secret|password|key.*=|token" configs/aos.toml | head -50
    else
        warn "Main config not found at configs/aos.toml"
    fi
    echo

    # Git Status
    separator
    echo "=== GIT STATUS ==="
    separator

    if [ -d .git ]; then
        echo "Current branch:"
        git branch --show-current
        echo
        echo "Recent commits:"
        git log --oneline -5
        echo
        echo "Modified files:"
        git status --short
    else
        warn "Not a git repository"
    fi
    echo

    # Network Connectivity
    separator
    echo "=== NETWORK CONNECTIVITY ==="
    separator

    echo "Testing localhost connectivity:"
    if ping -c 1 localhost >/dev/null 2>&1; then
        success "localhost reachable"
    else
        error "localhost unreachable"
    fi
    echo

    echo "Testing API connectivity:"
    if curl -f -s -m 5 "$API_BASE/healthz" >/dev/null 2>&1; then
        success "API reachable"
    else
        error "API unreachable"
    fi
    echo

    # Feature Flags
    separator
    echo "=== FEATURE FLAGS ==="
    separator

    if [ -f Cargo.toml ]; then
        echo "Workspace features:"
        grep -A 10 "\[workspace.dependencies\]" Cargo.toml | grep -E "features|mlx|metal|coreml" || echo "No specific features found"
    fi
    echo

    # Recent System Events (macOS)
    if [[ "$OSTYPE" == "darwin"* ]]; then
        separator
        echo "=== RECENT SYSTEM EVENTS (macOS) ==="
        separator

        echo "Recent crashes:"
        ls -lt ~/Library/Logs/DiagnosticReports/*adapteros* 2>/dev/null | head -5 || echo "No recent crashes"
        echo
    fi

    # Security Checks
    separator
    echo "=== SECURITY CHECKS ==="
    separator

    echo "Path validation (should not include /tmp):"
    if grep -r "/tmp" configs/ 2>/dev/null | grep -v "^Binary"; then
        error "Found /tmp references in config (not allowed)"
    else
        success "No /tmp references in config"
    fi
    echo

    echo "Foreign key constraints:"
    if [ -f "$DB_PATH" ]; then
        fk_status=$(sqlite3 "$DB_PATH" "PRAGMA foreign_keys;" 2>/dev/null)
        if [ "$fk_status" = "1" ]; then
            success "Foreign key constraints enabled"
        else
            error "Foreign key constraints disabled (should be enabled)"
        fi
    fi
    echo

    # Recommendations
    separator
    echo "=== RECOMMENDATIONS ==="
    separator

    recommendations=()

    # Check if service is running
    if ! ps aux | grep -E "adapteros-server" | grep -v grep >/dev/null; then
        recommendations+=("Service is not running. Start with: make dev")
    fi

    # Check for stub backend
    if [ -f "$LOG_FILE" ] && grep -q "stub.*active" "$LOG_FILE"; then
        recommendations+=("Stub backend detected. Build with real backend: cargo build --features mlx-backend")
    fi

    # Check disk space
    disk_usage=$(df . | tail -1 | awk '{print $5}' | sed 's/%//')
    if [ "$disk_usage" -gt 90 ]; then
        recommendations+=("Disk usage is high ($disk_usage%). Clean up old logs and data.")
    fi

    # Check log file size
    if [ -f "$LOG_FILE" ]; then
        log_size=$(ls -l "$LOG_FILE" | awk '{print $5}')
        if [ "$log_size" -gt 104857600 ]; then  # 100MB
            recommendations+=("Log file is large (>100MB). Consider rotation.")
        fi
    fi

    # Check WAL size
    if [ -f "${DB_PATH}-wal" ]; then
        wal_size=$(ls -l "${DB_PATH}-wal" | awk '{print $5}')
        if [ "$wal_size" -gt 10485760 ]; then  # 10MB
            recommendations+=("WAL file is large (>10MB). Run: sqlite3 var/aos-cp.sqlite3 'PRAGMA wal_checkpoint(TRUNCATE);'")
        fi
    fi

    if [ ${#recommendations[@]} -eq 0 ]; then
        success "No critical recommendations"
    else
        echo "Found ${#recommendations[@]} recommendation(s):"
        for rec in "${recommendations[@]}"; do
            echo "  - $rec"
        done
    fi
    echo

    # Summary
    separator
    echo "=== DIAGNOSTIC SUMMARY ==="
    separator

    echo "Health Status:"
    if curl -f -s -m 5 "$API_BASE/healthz" >/dev/null 2>&1; then
        success "System is responsive"
    else
        error "System is not responsive"
    fi
    echo

    echo "Components:"
    components=0
    issues=0

    if ps aux | grep -E "adapteros-server" | grep -v grep >/dev/null; then
        success "Control plane: Running"
        components=$((components + 1))
    else
        error "Control plane: Not running"
        issues=$((issues + 1))
    fi

    if [ -f "$DB_PATH" ]; then
        success "Database: Present"
        components=$((components + 1))
    else
        error "Database: Missing"
        issues=$((issues + 1))
    fi

    if ls var/run/aos/*/worker.sock >/dev/null 2>&1; then
        success "Worker: Connected"
        components=$((components + 1))
    else
        error "Worker: Not connected"
        issues=$((issues + 1))
    fi

    echo
    echo "Overall Status: $components/$((components + issues)) components healthy"

    if [ $issues -eq 0 ]; then
        success "System appears healthy"
    elif [ $issues -eq 1 ]; then
        warn "System has 1 issue"
    else
        error "System has $issues issues"
    fi
    echo

    # Footer
    echo "==================================================================="
    echo "End of diagnostic report"
    echo "Generated: $(date)"
    echo "==================================================================="

} | tee "$REPORT_FILE"

# Print conclusion
echo
info "Diagnostic report saved to: $REPORT_FILE"
echo

# Suggest next steps based on findings
if ! curl -f -s -m 5 "$API_BASE/healthz" >/dev/null 2>&1; then
    echo -e "${YELLOW}NEXT STEPS:${NC}"
    echo "1. Start the service: make dev"
    echo "2. Check logs: tail -50 var/aos-cp.log"
    echo "3. Verify database: sqlite3 var/aos-cp.sqlite3 'SELECT 1;'"
    echo
fi
