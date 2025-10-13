#!/usr/bin/env bash
# Script to update all Cargo.toml files with new crate names
# Part of Phase 1: Naming Unification

set -e

echo "Updating Cargo.toml files with new crate names..."

# Find all Cargo.toml files and update them
find_and_update() {
    local pattern="$1"
    local files
    
    # Collect all Cargo.toml files
    files=$(find . -name "Cargo.toml" -type f | grep -v target | grep -v ".git")
    
    for file in $files; do
        if [ -f "$file" ]; then
            # Create backup
            cp "$file" "${file}.bak"
            
            # Core infrastructure renames
            sed -i '' 's/mplora-core/adapteros-core/g' "$file"
            sed -i '' 's/mplora-crypto/adapteros-crypto/g' "$file"
            sed -i '' 's/mplora-manifest/adapteros-manifest/g' "$file"
            sed -i '' 's/mplora-registry/adapteros-registry/g' "$file"
            sed -i '' 's/mplora-artifacts/adapteros-artifacts/g' "$file"
            sed -i '' 's/mplora-db/adapteros-db/g' "$file"
            sed -i '' 's/mplora-git/adapteros-git/g' "$file"
            sed -i '' 's/mplora-node/adapteros-node/g' "$file"
            
            # LoRA feature module renames (do these BEFORE general mplora- patterns)
            sed -i '' 's/mplora-kernel-api/adapteros-lora-kernel-api/g' "$file"
            sed -i '' 's/mplora-kernel-mtl/adapteros-lora-kernel-mtl/g' "$file"
            sed -i '' 's/mplora-kernel-prof/adapteros-lora-kernel-prof/g' "$file"
            sed -i '' 's/mplora-router/adapteros-lora-router/g' "$file"
            sed -i '' 's/mplora-rag/adapteros-lora-rag/g' "$file"
            sed -i '' 's/mplora-worker/adapteros-lora-worker/g' "$file"
            sed -i '' 's/mplora-plan/adapteros-lora-plan/g' "$file"
            sed -i '' 's/mplora-quant/adapteros-lora-quant/g' "$file"
            sed -i '' 's/mplora-lifecycle/adapteros-lora-lifecycle/g' "$file"
            sed -i '' 's/mplora-mlx/adapteros-lora-mlx/g' "$file"
            
            # Control plane renames
            sed -i '' 's/mplora-server-api/adapteros-server-api/g' "$file"
            sed -i '' 's/mplora-server/adapteros-server/g' "$file"
            sed -i '' 's/mplora-orchestrator/adapteros-orchestrator/g' "$file"
            
            # Security & policy renames
            sed -i '' 's/mplora-policy/adapteros-policy/g' "$file"
            sed -i '' 's/mplora-secd/adapteros-secd/g' "$file"
            sed -i '' 's/mplora-sbom/adapteros-sbom/g' "$file"
            
            # Observability renames
            sed -i '' 's/mplora-telemetry/adapteros-telemetry/g' "$file"
            sed -i '' 's/mplora-system-metrics/adapteros-system-metrics/g' "$file"
            sed -i '' 's/mplora-profiler/adapteros-profiler/g' "$file"
            sed -i '' 's/mplora-metrics-exporter/adapteros-metrics-exporter/g' "$file"
            
            # Interface renames
            sed -i '' 's/mplora-cli/adapteros-cli/g' "$file"
            sed -i '' 's/mplora-api/adapteros-api/g' "$file"
            sed -i '' 's/mplora-client/adapteros-client/g' "$file"
            sed -i '' 's/mplora-chat/adapteros-chat/g' "$file"
            
            # Infrastructure renames
            sed -i '' 's/mplora-codegraph/adapteros-codegraph/g' "$file"
            
            echo "Updated: $file"
        fi
    done
}

# Run the updates
find_and_update

# Clean up backup files
find . -name "Cargo.toml.bak" -type f -delete

echo "Done! All Cargo.toml files updated."
echo "Run 'cargo check --workspace' to verify."
