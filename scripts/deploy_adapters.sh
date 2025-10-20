#!/bin/bash
# AdapterOS Adapter Deployment Script
# Supports both directory-based and .aos single-file adapters
# Usage: ./deploy_adapters.sh <adapter_path1> [adapter_path2 ...]

set -e

ADAPTERS_DIR="/opt/adapteros/adapters"
BACKUP_DIR="/opt/adapteros/adapters.backup.$(date +%Y%m%d_%H%M%S)"

deploy_directory_adapter() {
    local adapter_dir="$1"
    local adapter_name=$(basename "$adapter_dir")
    
    echo "Deploying directory adapter: $adapter_name"
    
    # Backup existing if present
    if [ -d "$ADAPTERS_DIR/$adapter_name" ]; then
        echo "Backing up existing adapter: $adapter_name"
        cp -r "$ADAPTERS_DIR/$adapter_name" "$BACKUP_DIR/$adapter_name"
    fi
    
    # Copy directory
    cp -r "$adapter_dir" "$ADAPTERS_DIR/"
    
    # Register
    aosctl adapters register --path "$ADAPTERS_DIR/$adapter_name" || {
        echo "ERROR: Failed to register directory adapter $adapter_name"
        exit 1
    }
    
    echo "✓ Deployed directory adapter: $adapter_name"
}

deploy_aos_adapter() {
    local aos_file="$1"
    local adapter_name=$(basename "$aos_file" .aos)
    
    echo "Deploying .aos adapter: $adapter_name"
    
    # Verify .aos file
    aosctl aos verify --path "$aos_file" || {
        echo "ERROR: .aos file verification failed for $aos_file"
        exit 1
    }
    
    # Backup existing if present
    if [ -f "$ADAPTERS_DIR/$adapter_name.aos" ]; then
        echo "Backing up existing .aos adapter: $adapter_name"
        cp "$ADAPTERS_DIR/$adapter_name.aos" "$BACKUP_DIR/$adapter_name.aos"
    fi
    
    # Copy .aos file
    cp "$aos_file" "$ADAPTERS_DIR/"
    
    # Load into registry
    aosctl aos load --path "$ADAPTERS_DIR/$adapter_name.aos" || {
        echo "ERROR: Failed to load .aos adapter $adapter_name"
        exit 1
    }
    
    echo "✓ Deployed .aos adapter: $adapter_name"
}

# Main deployment loop
if [ $# -eq 0 ]; then
    echo "Usage: $0 <adapter_path1> [adapter_path2 ...]"
    echo "Supports: adapter directories, *.safetensors files, *.aos files"
    exit 1
fi

# Create adapters dir if not exists
mkdir -p "$ADAPTERS_DIR"

for adapter in "$@"; do
    if [ ! -e "$adapter" ]; then
        echo "ERROR: Adapter path not found: $adapter"
        exit 1
    fi
    
    if [ -d "$adapter" ]; then
        deploy_directory_adapter "$adapter"
    elif [[ "$adapter" == *.aos ]]; then
        deploy_aos_adapter "$adapter"
    else
        # Assume weights.safetensors or similar
        local dir=$(dirname "$adapter")
        local name=$(basename "$dir")
        deploy_directory_adapter "$dir"
    fi
done

echo "Deployment complete. Backup available at $BACKUP_DIR"
