#!/bin/bash
# Script to rename all aos_* references to mplora_* in Rust source files

set -e

echo "Updating import statements in Rust files..."

# Find all .rs files in crates/
find crates -name "*.rs" | while read -r file; do
    # Update use statements
    sed -i '' 's/use aos_core/use mplora_core/g' "$file"
    sed -i '' 's/use aos_crypto/use mplora_crypto/g' "$file"
    sed -i '' 's/use aos_manifest/use mplora_manifest/g' "$file"
    sed -i '' 's/use aos_registry/use mplora_registry/g' "$file"
    sed -i '' 's/use aos_artifacts/use mplora_artifacts/g' "$file"
    sed -i '' 's/use aos_kernel_api/use mplora_kernel_api/g' "$file"
    sed -i '' 's/use aos_kernel_mtl/use mplora_kernel_mtl/g' "$file"
    sed -i '' 's/use aos_plan/use mplora_plan/g' "$file"
    sed -i '' 's/use aos_router/use mplora_router/g' "$file"
    sed -i '' 's/use aos_rag/use mplora_rag/g' "$file"
    sed -i '' 's/use aos_worker/use mplora_worker/g' "$file"
    sed -i '' 's/use aos_telemetry/use mplora_telemetry/g' "$file"
    sed -i '' 's/use aos_policy/use mplora_policy/g' "$file"
    sed -i '' 's/use aos_api/use mplora_api/g' "$file"
    sed -i '' 's/use aos_sbom/use mplora_sbom/g' "$file"
    sed -i '' 's/use aos_quant/use mplora_quant/g' "$file"
    sed -i '' 's/use aos_cp_api/use mplora_server_api/g' "$file"
    sed -i '' 's/use aos_cp_db/use mplora_db/g' "$file"
    sed -i '' 's/use aos_cp_client/use mplora_client/g' "$file"
    sed -i '' 's/use aos_node/use mplora_node/g' "$file"
    
    # Update path references (aos::* to mplora::*)
    sed -i '' 's/aos_core::/mplora_core::/g' "$file"
    sed -i '' 's/aos_crypto::/mplora_crypto::/g' "$file"
    sed -i '' 's/aos_manifest::/mplora_manifest::/g' "$file"
    sed -i '' 's/aos_registry::/mplora_registry::/g' "$file"
    sed -i '' 's/aos_artifacts::/mplora_artifacts::/g' "$file"
    sed -i '' 's/aos_kernel_api::/mplora_kernel_api::/g' "$file"
    sed -i '' 's/aos_kernel_mtl::/mplora_kernel_mtl::/g' "$file"
    sed -i '' 's/aos_plan::/mplora_plan::/g' "$file"
    sed -i '' 's/aos_router::/mplora_router::/g' "$file"
    sed -i '' 's/aos_rag::/mplora_rag::/g' "$file"
    sed -i '' 's/aos_worker::/mplora_worker::/g' "$file"
    sed -i '' 's/aos_telemetry::/mplora_telemetry::/g' "$file"
    sed -i '' 's/aos_policy::/mplora_policy::/g' "$file"
    sed -i '' 's/aos_api::/mplora_api::/g' "$file"
    sed -i '' 's/aos_sbom::/mplora_sbom::/g' "$file"
    sed -i '' 's/aos_quant::/mplora_quant::/g' "$file"
    sed -i '' 's/aos_cp_api::/mplora_server_api::/g' "$file"
    sed -i '' 's/aos_cp_db::/mplora_db::/g' "$file"
    sed -i '' 's/aos_cp_client::/mplora_client::/g' "$file"
    sed -i '' 's/aos_node::/mplora_node::/g' "$file"
    
    # Remove references to deleted crates
    sed -i '' '/use aos_cdp/d' "$file"
    sed -i '' '/use aos_codegraph/d' "$file"
    sed -i '' '/use aos_indices/d' "$file"
    sed -i '' '/aos_cdp::/d' "$file"
    sed -i '' '/aos_codegraph::/d' "$file"
    sed -i '' '/aos_indices::/d' "$file"
done

echo "Done! Import statements updated."
echo "Note: You may need to manually fix some references to deleted crates."
