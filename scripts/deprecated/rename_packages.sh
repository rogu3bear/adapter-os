#!/bin/bash
# Script to rename all aos-* references to mplora-* in Cargo.toml files

set -e

echo "Updating package names in Cargo.toml files..."

# Find all Cargo.toml files in crates/
find crates -name "Cargo.toml" | while read -r file; do
    echo "Processing: $file"
    
    # Update package name
    sed -i '' 's/^name = "aos-/name = "mplora-/g' "$file"
    
    # Update dependencies
    sed -i '' 's/aos-core/mplora-core/g' "$file"
    sed -i '' 's/aos-crypto/mplora-crypto/g' "$file"
    sed -i '' 's/aos-manifest/mplora-manifest/g' "$file"
    sed -i '' 's/aos-registry/mplora-registry/g' "$file"
    sed -i '' 's/aos-artifacts/mplora-artifacts/g' "$file"
    sed -i '' 's/aos-kernel-api/mplora-kernel-api/g' "$file"
    sed -i '' 's/aos-kernel-mtl/mplora-kernel-mtl/g' "$file"
    sed -i '' 's/aos-plan/mplora-plan/g' "$file"
    sed -i '' 's/aos-router/mplora-router/g' "$file"
    sed -i '' 's/aos-rag/mplora-rag/g' "$file"
    sed -i '' 's/aos-worker/mplora-worker/g' "$file"
    sed -i '' 's/aos-telemetry/mplora-telemetry/g' "$file"
    sed -i '' 's/aos-policy/mplora-policy/g' "$file"
    sed -i '' 's/aos-cli/mplora-cli/g' "$file"
    sed -i '' 's/aos-api/mplora-api/g' "$file"
    sed -i '' 's/aos-sbom/mplora-sbom/g' "$file"
    sed -i '' 's/aos-quant/mplora-quant/g' "$file"
    sed -i '' 's/aos-cp-api/mplora-server-api/g' "$file"
    sed -i '' 's/aos-cp-db/mplora-db/g' "$file"
    sed -i '' 's/aos-cp-client/mplora-client/g' "$file"
    sed -i '' 's/aos-cp/mplora-server/g' "$file"
    sed -i '' 's/aos-node/mplora-node/g' "$file"
    
    # Remove references to deleted crates
    sed -i '' '/aos-cdp/d' "$file"
    sed -i '' '/aos-codegraph/d' "$file"
    sed -i '' '/aos-indices/d' "$file"
    sed -i '' '/aos-cp-jobs/d' "$file"
    sed -i '' '/aos-cp-telemetry/d' "$file"
    sed -i '' '/aos-ui-web/d' "$file"
    sed -i '' '/aos-ui-common/d' "$file"
    sed -i '' '/aos-chat/d' "$file"
done

echo "Done! Package names updated."
