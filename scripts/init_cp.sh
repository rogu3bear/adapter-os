#!/bin/bash
# Initialize AdapterOS Control Plane

set -e

echo "AdapterOS Control Plane Initialization"
echo "======================================="

# Create directories
echo "Creating directories..."
mkdir -p var
mkdir -p /srv/aos/artifacts
mkdir -p /srv/aos/bundles

# Check configuration
if [ ! -f "configs/cp.toml" ]; then
    echo "Error: configs/cp.toml not found"
    exit 1
fi

# Generate JWT secret if needed
if grep -q "CHANGE_ME_IN_PRODUCTION" configs/cp.toml; then
    echo "Warning: JWT secret not set in configs/cp.toml"
    echo "Generate a random secret with: openssl rand -base64 48"
fi

# Build control plane
echo "Building control plane..."
cargo build --release --bin aos-cp

# Run migrations
echo "Running database migrations..."
./target/release/aos-cp --config configs/cp.toml --migrate-only

echo ""
echo "Control plane initialized successfully!"
echo ""
echo "To start the control plane:"
echo "  ./target/release/aos-cp --config configs/cp.toml"
echo ""
echo "To create an admin user (after starting):"
echo "  cargo run --bin aosctl user create --email admin@example.com --role admin"
