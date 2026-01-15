#!/bin/bash
# Dependency Consolidation Script for adapterOS
# Generated: 2025-11-21
# Run from workspace root: ./scripts/consolidate_deps.sh

set -e

echo "=== adapterOS Dependency Consolidation ==="
echo ""

# Colors for output
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
NC='\033[0m' # No Color

# Phase 1: Critical Version Upgrades
echo -e "${YELLOW}Phase 1: Critical Version Upgrades${NC}"

# 1a. Upgrade axum 0.7 to 0.8 in service-supervisor
echo "  [1/4] Upgrading axum 0.7 → 0.8 in adapteros-service-supervisor..."
sed -i '' 's/axum = { version = "0.7"/axum = { version = "0.8"/' crates/adapteros-service-supervisor/Cargo.toml

# 1b. Fix sqlx runtime feature in codegraph
echo "  [2/4] Fixing sqlx runtime-tokio-rustls → runtime-tokio in adapteros-codegraph..."
sed -i '' 's/runtime-tokio-rustls/runtime-tokio/' crates/adapteros-codegraph/Cargo.toml

# 1c. Upgrade reqwest 0.11 to 0.12
echo "  [3/4] Upgrading reqwest 0.11 → 0.12 in xtask..."
sed -i '' 's/reqwest = { version = "0.11"/reqwest = { version = "0.12"/' xtask/Cargo.toml

echo "  [4/4] Upgrading reqwest 0.11 → 0.12 in adapteros-testing..."
sed -i '' 's/reqwest = { version = "0.11"/reqwest = { version = "0.12"/' crates/adapteros-testing/Cargo.toml

echo "  [5/4] Upgrading reqwest 0.11 → 0.12 in adapteros-client..."
sed -i '' 's/reqwest = { version = "0.11"/reqwest = { version = "0.12"/' crates/adapteros-client/Cargo.toml

echo -e "${GREEN}Phase 1 complete.${NC}"
echo ""

# Phase 2: Workspace tokio consolidation
echo -e "${YELLOW}Phase 2: Workspace tokio consolidation${NC}"

TOKIO_CRATES=(
    "crates/adapteros-core/Cargo.toml"
    "crates/adapteros-error-recovery/Cargo.toml"
    "crates/adapteros-storage/Cargo.toml"
    "crates/adapteros-concurrent-fs/Cargo.toml"
    "crates/adapteros-patch/Cargo.toml"
    "crates/adapteros-temp/Cargo.toml"
    "crates/adapteros-server-api/Cargo.toml"
    "crates/adapteros-service-supervisor/Cargo.toml"
)

for crate in "${TOKIO_CRATES[@]}"; do
    if [ -f "$crate" ]; then
        echo "  Converting tokio to workspace in $crate..."
        # Replace version = "1.0" with workspace = true
        sed -i '' 's/tokio = { version = "1.0"/tokio = { workspace = true/' "$crate"
        sed -i '' 's/tokio = { version = "1.35"/tokio = { workspace = true/' "$crate"
    fi
done

echo -e "${GREEN}Phase 2 complete.${NC}"
echo ""

# Phase 3: Workspace serde consolidation
echo -e "${YELLOW}Phase 3: Workspace serde consolidation${NC}"

SERDE_CRATES=(
    "crates/adapteros-error-recovery/Cargo.toml"
    "crates/adapteros-storage/Cargo.toml"
    "crates/adapteros-concurrent-fs/Cargo.toml"
    "crates/adapteros-patch/Cargo.toml"
    "crates/adapteros-temp/Cargo.toml"
    "crates/adapteros-single-file-adapter/Cargo.toml"
    "crates/adapteros-server-api/Cargo.toml"
    "crates/adapteros-service-supervisor/Cargo.toml"
)

for crate in "${SERDE_CRATES[@]}"; do
    if [ -f "$crate" ]; then
        echo "  Converting serde to workspace in $crate..."
        # Replace direct serde specification with workspace
        sed -i '' 's/serde = { version = "1.0", features = \["derive"\] }/serde = { workspace = true }/' "$crate"
    fi
done

echo -e "${GREEN}Phase 3 complete.${NC}"
echo ""

# Verify changes
echo -e "${YELLOW}Verifying changes...${NC}"
echo ""

echo "Checking for remaining version = \"0.7\" axum..."
grep -r 'axum = { version = "0.7"' crates/ 2>/dev/null || echo "  ✓ No axum 0.7 found"

echo "Checking for remaining reqwest 0.11..."
grep -r 'reqwest = { version = "0.11"' . --include="Cargo.toml" 2>/dev/null || echo "  ✓ No reqwest 0.11 found"

echo "Checking for runtime-tokio-rustls..."
grep -r 'runtime-tokio-rustls' crates/ 2>/dev/null || echo "  ✓ No runtime-tokio-rustls found"

echo ""
echo -e "${GREEN}=== Consolidation Complete ===${NC}"
echo ""
echo "Next steps:"
echo "  1. Run: cargo build --workspace"
echo "  2. Run: cargo test --workspace"
echo "  3. Review docs/DEPENDENCY_CONSOLIDATION.md for type deduplication"
