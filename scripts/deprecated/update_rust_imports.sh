#!/usr/bin/env bash
# Script to update Rust source files with new crate import names
# Part of Phase 1: Naming Unification

set -e

echo "Updating Rust source files with new import names..."

# Find all .rs files and update imports
find crates -name "*.rs" -type f | while read -r file; do
    # Create backup
    cp "$file" "${file}.bak"
    
    # Update use statements and external crate references
    # Core infrastructure
    sed -i '' 's/use mplora_core::/use adapteros_core::/g' "$file"
    sed -i '' 's/extern crate mplora_core;/extern crate adapteros_core;/g' "$file"
    sed -i '' 's/mplora_core::/adapteros_core::/g' "$file"
    
    sed -i '' 's/use mplora_crypto::/use adapteros_crypto::/g' "$file"
    sed -i '' 's/mplora_crypto::/adapteros_crypto::/g' "$file"
    
    sed -i '' 's/use mplora_manifest::/use adapteros_manifest::/g' "$file"
    sed -i '' 's/mplora_manifest::/adapteros_manifest::/g' "$file"
    
    sed -i '' 's/use mplora_registry::/use adapteros_registry::/g' "$file"
    sed -i '' 's/mplora_registry::/adapteros_registry::/g' "$file"
    
    sed -i '' 's/use mplora_artifacts::/use adapteros_artifacts::/g' "$file"
    sed -i '' 's/mplora_artifacts::/adapteros_artifacts::/g' "$file"
    
    sed -i '' 's/use mplora_db::/use adapteros_db::/g' "$file"
    sed -i '' 's/mplora_db::/adapteros_db::/g' "$file"
    
    sed -i '' 's/use mplora_git::/use adapteros_git::/g' "$file"
    sed -i '' 's/mplora_git::/adapteros_git::/g' "$file"
    
    sed -i '' 's/use mplora_node::/use adapteros_node::/g' "$file"
    sed -i '' 's/mplora_node::/adapteros_node::/g' "$file"
    
    # LoRA feature module (do these BEFORE general patterns)
    sed -i '' 's/use mplora_kernel_api::/use adapteros_lora_kernel_api::/g' "$file"
    sed -i '' 's/mplora_kernel_api::/adapteros_lora_kernel_api::/g' "$file"
    
    sed -i '' 's/use mplora_kernel_mtl::/use adapteros_lora_kernel_mtl::/g' "$file"
    sed -i '' 's/mplora_kernel_mtl::/adapteros_lora_kernel_mtl::/g' "$file"
    
    sed -i '' 's/use mplora_kernel_prof::/use adapteros_lora_kernel_prof::/g' "$file"
    sed -i '' 's/mplora_kernel_prof::/adapteros_lora_kernel_prof::/g' "$file"
    
    sed -i '' 's/use mplora_router::/use adapteros_lora_router::/g' "$file"
    sed -i '' 's/mplora_router::/adapteros_lora_router::/g' "$file"
    
    sed -i '' 's/use mplora_rag::/use adapteros_lora_rag::/g' "$file"
    sed -i '' 's/mplora_rag::/adapteros_lora_rag::/g' "$file"
    
    sed -i '' 's/use mplora_worker::/use adapteros_lora_worker::/g' "$file"
    sed -i '' 's/mplora_worker::/adapteros_lora_worker::/g' "$file"
    
    sed -i '' 's/use mplora_plan::/use adapteros_lora_plan::/g' "$file"
    sed -i '' 's/mplora_plan::/adapteros_lora_plan::/g' "$file"
    
    sed -i '' 's/use mplora_quant::/use adapteros_lora_quant::/g' "$file"
    sed -i '' 's/mplora_quant::/adapteros_lora_quant::/g' "$file"
    
    sed -i '' 's/use mplora_lifecycle::/use adapteros_lora_lifecycle::/g' "$file"
    sed -i '' 's/mplora_lifecycle::/adapteros_lora_lifecycle::/g' "$file"
    
    sed -i '' 's/use mplora_mlx::/use adapteros_lora_mlx::/g' "$file"
    sed -i '' 's/mplora_mlx::/adapteros_lora_mlx::/g' "$file"
    
    # Control plane
    sed -i '' 's/use mplora_server_api::/use adapteros_server_api::/g' "$file"
    sed -i '' 's/mplora_server_api::/adapteros_server_api::/g' "$file"
    
    sed -i '' 's/use mplora_server::/use adapteros_server::/g' "$file"
    sed -i '' 's/mplora_server::/adapteros_server::/g' "$file"
    
    sed -i '' 's/use mplora_orchestrator::/use adapteros_orchestrator::/g' "$file"
    sed -i '' 's/mplora_orchestrator::/adapteros_orchestrator::/g' "$file"
    
    # Security & policy
    sed -i '' 's/use mplora_policy::/use adapteros_policy::/g' "$file"
    sed -i '' 's/mplora_policy::/adapteros_policy::/g' "$file"
    
    sed -i '' 's/use mplora_secd::/use adapteros_secd::/g' "$file"
    sed -i '' 's/mplora_secd::/adapteros_secd::/g' "$file"
    
    sed -i '' 's/use mplora_sbom::/use adapteros_sbom::/g' "$file"
    sed -i '' 's/mplora_sbom::/adapteros_sbom::/g' "$file"
    
    # Observability
    sed -i '' 's/use mplora_telemetry::/use adapteros_telemetry::/g' "$file"
    sed -i '' 's/mplora_telemetry::/adapteros_telemetry::/g' "$file"
    
    sed -i '' 's/use mplora_system_metrics::/use adapteros_system_metrics::/g' "$file"
    sed -i '' 's/mplora_system_metrics::/adapteros_system_metrics::/g' "$file"
    
    sed -i '' 's/use mplora_profiler::/use adapteros_profiler::/g' "$file"
    sed -i '' 's/mplora_profiler::/adapteros_profiler::/g' "$file"
    
    sed -i '' 's/use mplora_metrics_exporter::/use adapteros_metrics_exporter::/g' "$file"
    sed -i '' 's/mplora_metrics_exporter::/adapteros_metrics_exporter::/g' "$file"
    
    # Interfaces
    sed -i '' 's/use mplora_cli::/use adapteros_cli::/g' "$file"
    sed -i '' 's/mplora_cli::/adapteros_cli::/g' "$file"
    
    sed -i '' 's/use mplora_api::/use adapteros_api::/g' "$file"
    sed -i '' 's/mplora_api::/adapteros_api::/g' "$file"
    
    sed -i '' 's/use mplora_client::/use adapteros_client::/g' "$file"
    sed -i '' 's/mplora_client::/adapteros_client::/g' "$file"
    
    sed -i '' 's/use mplora_chat::/use adapteros_chat::/g' "$file"
    sed -i '' 's/mplora_chat::/adapteros_chat::/g' "$file"
    
    # Infrastructure
    sed -i '' 's/use mplora_codegraph::/use adapteros_codegraph::/g' "$file"
    sed -i '' 's/mplora_codegraph::/adapteros_codegraph::/g' "$file"
    
    # Update crate references in doc comments and string literals (selective)
    # Only update in documentation paths
    sed -i '' 's/crates\/mplora-core/crates\/adapteros-core/g' "$file"
    sed -i '' 's/crates\/mplora-crypto/crates\/adapteros-crypto/g' "$file"
    sed -i '' 's/crates\/mplora-worker/crates\/adapteros-lora-worker/g' "$file"
done

# Clean up backup files
find crates -name "*.rs.bak" -type f -delete

echo "Done! All Rust source files updated."
echo "Run 'cargo check --workspace' to verify."

