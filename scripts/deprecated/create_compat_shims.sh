#!/usr/bin/env bash
# Script to create compatibility shim crates
# Part of Phase 1: Naming Unification

set -e

echo "Creating compatibility shim crates..."

# Create compat directory
mkdir -p crates/compat

# Function to create a shim crate
create_shim() {
    local old_name="$1"
    local new_name="$2"
    local shim_dir="crates/compat/${old_name}"
    
    echo "Creating shim: $old_name -> $new_name"
    
    mkdir -p "$shim_dir/src"
    
    # Create Cargo.toml
    cat > "$shim_dir/Cargo.toml" <<EOF
[package]
name = "${old_name}"
version = "0.2.0"
edition = "2021"
authors = ["James KC Auchterlonie <vats-springs0m@icloud.com>"]
license = "MIT OR Apache-2.0"

[dependencies]
${new_name} = { path = "../../${new_name}" }
EOF

    # Create lib.rs with deprecation warning
    local underscore_old=$(echo "$old_name" | tr '-' '_')
    local underscore_new=$(echo "$new_name" | tr '-' '_')
    
    cat > "$shim_dir/src/lib.rs" <<EOF
#![deprecated(
    since = "0.2.0",
    note = "Use ${new_name} instead. This compatibility crate will be removed in 0.3.0."
)]

//! Compatibility shim for \`${old_name}\`
//!
//! This crate has been renamed to \`${new_name}\`.
//! Please update your \`Cargo.toml\` to use the new name:
//!
//! \`\`\`toml
//! [dependencies]
//! ${new_name} = "0.2"
//! \`\`\`

pub use ${underscore_new}::*;
EOF
}

# Create shims for all renamed crates
# Core infrastructure
create_shim "mplora-core" "adapteros-core"
create_shim "mplora-crypto" "adapteros-crypto"
create_shim "mplora-manifest" "adapteros-manifest"
create_shim "mplora-registry" "adapteros-registry"
create_shim "mplora-artifacts" "adapteros-artifacts"
create_shim "mplora-db" "adapteros-db"
create_shim "mplora-git" "adapteros-git"
create_shim "mplora-node" "adapteros-node"

# LoRA feature module
create_shim "mplora-kernel-api" "adapteros-lora-kernel-api"
create_shim "mplora-kernel-mtl" "adapteros-lora-kernel-mtl"
create_shim "mplora-kernel-prof" "adapteros-lora-kernel-prof"
create_shim "mplora-router" "adapteros-lora-router"
create_shim "mplora-rag" "adapteros-lora-rag"
create_shim "mplora-worker" "adapteros-lora-worker"
create_shim "mplora-plan" "adapteros-lora-plan"
create_shim "mplora-quant" "adapteros-lora-quant"
create_shim "mplora-lifecycle" "adapteros-lora-lifecycle"

# Control plane
create_shim "mplora-server" "adapteros-server"
create_shim "mplora-server-api" "adapteros-server-api"
create_shim "mplora-orchestrator" "adapteros-orchestrator"

# Security & policy
create_shim "mplora-policy" "adapteros-policy"
create_shim "mplora-secd" "adapteros-secd"
create_shim "mplora-sbom" "adapteros-sbom"

# Observability
create_shim "mplora-telemetry" "adapteros-telemetry"
create_shim "mplora-system-metrics" "adapteros-system-metrics"
create_shim "mplora-profiler" "adapteros-profiler"
create_shim "mplora-metrics-exporter" "adapteros-metrics-exporter"

# Interfaces
create_shim "mplora-cli" "adapteros-cli"
create_shim "mplora-api" "adapteros-api"
create_shim "mplora-client" "adapteros-client"
create_shim "mplora-chat" "adapteros-chat"

# Infrastructure
create_shim "mplora-codegraph" "adapteros-codegraph"

echo "Done! Created compatibility shims in crates/compat/"
echo "Add them to workspace members in root Cargo.toml"

