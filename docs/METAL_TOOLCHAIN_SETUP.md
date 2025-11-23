# Metal Toolchain Setup Guide

**Task:** H1 - Metal Kernel Compilation
**Version:** 1.0
**Date:** 2025-01-23
**Copyright:** © 2025 JKCA / James KC Auchterlonie. All rights reserved.

---

## Overview

The Metal backend in AdapterOS requires the Metal Toolchain to compile `.metal` shader files into `.metallib` binaries. This guide covers installation, troubleshooting, and verification of the Metal Toolchain.

**Related Documents:**
- [CLAUDE.md](../CLAUDE.md) - Multi-backend architecture
- [docs/ADR_MULTI_BACKEND_STRATEGY.md](ADR_MULTI_BACKEND_STRATEGY.md) - Backend selection rationale
- [docs/features/FEATURE-INVENTORY.md](features/FEATURE-INVENTORY.md) - H1 task details

---

## Quick Start

### Automated Installation

```bash
# Run the automated installer
./scripts/install-metal-toolchain.sh
```

The script will:
1. Check for Xcode installation
2. Verify Metal compiler availability
3. Download and install Metal Toolchain (if needed)
4. Verify installation with test compilation

### Manual Installation

If the automated script fails:

```bash
# Install Metal Toolchain via xcodebuild
xcodebuild -downloadComponent MetalToolchain
```

**Alternative (GUI):**
1. Open Xcode
2. Go to **Preferences > Components**
3. Install **Metal Toolchain**

---

## Prerequisites

### 1. Xcode Command Line Tools

**Check installation:**
```bash
xcode-select -p
# Expected: /Applications/Xcode.app/Contents/Developer
```

**Install if missing:**
```bash
xcode-select --install
```

### 2. Xcode Version Requirements

- **Minimum:** Xcode 14.0 (macOS 12.5+)
- **Recommended:** Xcode 15.0+ (macOS 13.0+)
- **Current:** Xcode 16.0+ (macOS 14.0+)

**Check version:**
```bash
xcodebuild -version
# Expected: Xcode 15.0 or later
```

### 3. macOS Version

- **Minimum:** macOS 12.5 (Monterey)
- **Recommended:** macOS 13.0+ (Ventura)
- **Optimal:** macOS 14.0+ (Sonoma) for ANE support

---

## Verification

### 1. Check Metal Compiler

```bash
# Find Metal compiler path
xcrun --find metal
# Expected: /Applications/Xcode.app/Contents/Developer/Toolchains/XcodeDefault.xctoolchain/usr/bin/metal
```

### 2. Test Compilation

Create a test shader:

```bash
# Create test directory
mkdir -p /tmp/metal-test
cd /tmp/metal-test

# Create test shader
cat > test.metal << 'EOF'
#include <metal_stdlib>
using namespace metal;

kernel void test_kernel(device float* data [[buffer(0)]]) {
    uint tid = threadgroup_position_in_grid.x;
    data[tid] = 1.0f;
}
EOF

# Compile to AIR (intermediate representation)
xcrun -sdk macosx metal -c test.metal -o test.air

# Link to metallib
xcrun -sdk macosx metallib test.air -o test.metallib

# Verify output
ls -lh test.metallib
# Expected: test.metallib file created
```

### 3. Verify AdapterOS Build

```bash
# Clean build to force Metal compilation
cargo clean -p adapteros-lora-kernel-mtl

# Build Metal kernel crate
cargo build -p adapteros-lora-kernel-mtl

# Expected output:
# cargo:warning=Kernel hash: <blake3-hash>
# Finished dev [unoptimized + debuginfo] target(s)
```

**Success indicators:**
- ✅ No "missing Metal Toolchain" errors
- ✅ `crates/adapteros-lora-kernel-mtl/shaders/adapteros_kernels.metallib` created
- ✅ `crates/adapteros-lora-kernel-mtl/shaders/kernel_hash.txt` contains BLAKE3 hash
- ✅ Build completes without panics

---

## Troubleshooting

### Error: "cannot execute tool 'metal' due to missing Metal Toolchain"

**Symptom:**
```
error: cannot execute tool 'metal' due to missing Metal Toolchain
use: xcodebuild -downloadComponent MetalToolchain
```

**Solutions:**

1. **Run automated installer:**
   ```bash
   ./scripts/install-metal-toolchain.sh
   ```

2. **Manual installation:**
   ```bash
   xcodebuild -downloadComponent MetalToolchain
   ```

3. **Check Xcode license:**
   ```bash
   sudo xcodebuild -license accept
   ```

4. **Reset Xcode developer directory:**
   ```bash
   sudo xcode-select --reset
   sudo xcode-select --switch /Applications/Xcode.app
   ```

### Error: "Metal compiler not found"

**Symptom:**
```
❌ ERROR: Metal compiler not found
```

**Solutions:**

1. **Install Xcode Command Line Tools:**
   ```bash
   xcode-select --install
   ```

2. **Verify Xcode installation:**
   ```bash
   xcode-select -p
   # Should output: /Applications/Xcode.app/Contents/Developer
   ```

3. **Install full Xcode (if needed):**
   - Download from [Mac App Store](https://apps.apple.com/us/app/xcode/id497799835)
   - Or [Apple Developer Downloads](https://developer.apple.com/download/)

### Build Fails with Metal Syntax Errors

**Symptom:**
```
error: use of undeclared identifier 'metal'
```

**Solutions:**

1. **Check Metal version:**
   ```bash
   xcrun --show-sdk-version
   # Minimum: 12.5
   ```

2. **Verify Metal standard:**
   - AdapterOS uses Metal 3.1 (`-std=metal3.1`)
   - Requires Xcode 14.0+ and macOS 12.5+

3. **Update Xcode:**
   ```bash
   softwareupdate --list
   softwareupdate --install "Xcode"
   ```

### Metallib Not Embedded in Binary

**Symptom:**
```
Error: Failed to load Metal library
```

**Solutions:**

1. **Check shaders directory:**
   ```bash
   ls -lh crates/adapteros-lora-kernel-mtl/shaders/
   # Should contain: adapteros_kernels.metallib
   ```

2. **Force rebuild:**
   ```bash
   cargo clean -p adapteros-lora-kernel-mtl
   cargo build -p adapteros-lora-kernel-mtl --verbose
   ```

3. **Verify build.rs ran:**
   ```bash
   # Check build output for:
   # cargo:warning=Kernel hash: <hash>
   ```

---

## CI/CD Integration

### GitHub Actions

Add Metal Toolchain installation to `.github/workflows/`:

```yaml
name: Build

on: [push, pull_request]

jobs:
  build:
    runs-on: macos-latest

    steps:
      - uses: actions/checkout@v4

      - name: Install Rust
        uses: dtolnay/rust-toolchain@stable

      - name: Install Metal Toolchain
        run: |
          ./scripts/install-metal-toolchain.sh

      - name: Build
        run: cargo build --release

      - name: Test
        run: cargo test --workspace
```

### Local Pre-Commit Hook

Create `.git/hooks/pre-commit`:

```bash
#!/bin/bash
# Verify Metal kernels compile before committing

set -e

# Check if Metal files changed
if git diff --cached --name-only | grep -q '\.metal$'; then
    echo "Metal files changed, verifying compilation..."
    cargo build -p adapteros-lora-kernel-mtl
fi
```

---

## Metal Kernel Architecture

### Build Process

```
┌─────────────────────────────────────────────────────────────┐
│ 1. Source Files (.metal)                                    │
│    metal/src/kernels/adapteros_kernels.metal               │
│    metal/src/kernels/common.metal                          │
│    metal/src/kernels/attention.metal                       │
│    metal/src/kernels/mlp.metal                             │
│    metal/src/kernels/flash_attention.metal                 │
│    metal/src/kernels/mplora.metal                          │
└─────────────────────────────────────────────────────────────┘
                            ↓
            xcrun -sdk macosx metal -c -std=metal3.1
                            ↓
┌─────────────────────────────────────────────────────────────┐
│ 2. Intermediate Representation (.air)                       │
│    adapteros_kernels.air                                   │
└─────────────────────────────────────────────────────────────┘
                            ↓
                xcrun -sdk macosx metallib
                            ↓
┌─────────────────────────────────────────────────────────────┐
│ 3. Metal Library (.metallib)                                │
│    crates/adapteros-lora-kernel-mtl/shaders/               │
│      adapteros_kernels.metallib (embedded in binary)       │
└─────────────────────────────────────────────────────────────┘
                            ↓
                  BLAKE3 hash + manifest signing
                            ↓
┌─────────────────────────────────────────────────────────────┐
│ 4. Runtime Loading                                          │
│    MTLDevice::newLibraryWithData()                         │
│    Kernel dispatch via Metal API                           │
└─────────────────────────────────────────────────────────────┘
```

### File Locations

| File Type | Location | Purpose |
|-----------|----------|---------|
| `.metal` sources | `metal/src/kernels/` | Shader source code |
| `.air` intermediate | `metal/src/kernels/` (temp) | Compilation artifact (deleted) |
| `.metallib` binary | `crates/adapteros-lora-kernel-mtl/shaders/` | Embedded in Rust binary |
| `kernel_hash.txt` | `crates/adapteros-lora-kernel-mtl/shaders/` | BLAKE3 hash for verification |
| `metallib_manifest.json` | `crates/adapteros-lora-kernel-mtl/manifests/` | Signed manifest |
| `metallib_manifest.json.sig` | `crates/adapteros-lora-kernel-mtl/manifests/` | Ed25519 signature |

---

## Performance Considerations

### Compilation Time

- **Cold build:** ~5-10 seconds (Metal kernels)
- **Incremental:** <1 second (no shader changes)

**Optimization tips:**
1. Use `cargo build -p adapteros-lora-kernel-mtl` for isolated builds
2. Leverage `cargo:rerun-if-changed` in `build.rs`
3. Cache `.metallib` files in CI/CD

### Runtime Performance

- **GPU dispatch:** <1ms per kernel launch
- **Metal overhead:** ~100µs (library loading, first launch)
- **ANE execution:** 2-3x faster than GPU (M1/M2/M3)

**Benchmarking:**
```bash
# Run Metal kernel benchmarks
cargo bench -p adapteros-lora-kernel-mtl
```

---

## Security Considerations

### Kernel Signing

All Metal kernels are signed with Ed25519:

1. **Build time:** `build.rs` generates manifest and signature
2. **Runtime:** `MetalBackend::new()` verifies signature before loading
3. **Test keys:** Deterministic seed (see `build.rs:203-208`)
4. **Production:** Use hardware-backed keys (SEP, KMS)

**Verification:**
```bash
# Check manifest signature
cat crates/adapteros-lora-kernel-mtl/manifests/metallib_manifest.json.sig

# Verify BLAKE3 hash
cat crates/adapteros-lora-kernel-mtl/shaders/kernel_hash.txt
```

### Determinism

- **Precompiled kernels:** Ensures deterministic execution
- **Hash verification:** Detects tampering or corruption
- **Signed manifests:** Cryptographic proof of authenticity

See [docs/DETERMINISTIC_EXECUTION.md](DETERMINISTIC_EXECUTION.md) for full details.

---

## Advanced Usage

### Custom Metal Shaders

To add new kernels:

1. **Create shader file:**
   ```bash
   touch metal/src/kernels/my_custom_kernel.metal
   ```

2. **Update build.rs:**
   ```rust
   println!("cargo:rerun-if-changed=../../metal/src/kernels/my_custom_kernel.metal");
   ```

3. **Include in main kernel:**
   ```cpp
   // In adapteros_kernels.metal
   #include "my_custom_kernel.metal"
   ```

4. **Rebuild:**
   ```bash
   cargo build -p adapteros-lora-kernel-mtl
   ```

### Debugging Metal Shaders

**Enable Metal validation layers:**
```bash
export MTL_DEBUG_LAYER=1
export MTL_SHADER_VALIDATION=1
cargo run -p adapteros-lora-kernel-mtl
```

**Capture GPU frame:**
1. Run Xcode
2. Open **Window > Devices and Simulators**
3. Select your Mac
4. Click **Capture Metal Frame**

**Inspect compiled kernels:**
```bash
# Disassemble metallib
xcrun metal-objdump -d crates/adapteros-lora-kernel-mtl/shaders/adapteros_kernels.metallib
```

---

## FAQ

### Q: Do I need the full Xcode app or just Command Line Tools?

**A:** You need the **full Xcode app**. The Metal Toolchain component is only available through `xcodebuild`, which requires Xcode.

### Q: Can I use the Metal backend on Intel Macs?

**A:** Yes, but with GPU-only execution (no ANE). ANE requires Apple Silicon (M1/M2/M3).

### Q: What if I'm developing on Linux/Windows?

**A:** Use the MLX backend instead. Metal is macOS-only. See [docs/MLX_INTEGRATION.md](MLX_INTEGRATION.md).

### Q: How often should I update the Metal Toolchain?

**A:** Update when you update Xcode. The toolchain version should match your Xcode version.

### Q: Can I cross-compile Metal shaders?

**A:** No. Metal compilation requires macOS. CI/CD must run on macOS runners.

---

## References

- [Metal Shading Language Specification](https://developer.apple.com/metal/Metal-Shading-Language-Specification.pdf)
- [Metal Best Practices Guide](https://developer.apple.com/documentation/metal/best_practices_for_metal_apps)
- [Apple Silicon Performance Guide](https://developer.apple.com/documentation/metal/gpu_features/understanding_gpu_family_4)
- [AdapterOS Multi-Backend Strategy](ADR_MULTI_BACKEND_STRATEGY.md)

---

## Maintenance

**Last Updated:** 2025-01-23
**Maintained by:** Team 7 (Platform & Tooling)
**Review Cycle:** Quarterly (or with Xcode major releases)

**Change Log:**
- 2025-01-23: Initial version (H1 task completion)

---

**Document Control:**
- **Version:** 1.0
- **Status:** Active
- **Related Tasks:** H1 (Metal Kernel Compilation)
- **Next Review:** 2025-04-23

© 2025 JKCA / James KC Auchterlonie. All rights reserved.
