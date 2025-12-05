# MLX Metal Device Access Troubleshooting

**Issue:** MLX runtime aborts with `NSRangeException` when `MTLCreateSystemDefaultDevice()` returns nil
**Root Cause:** Process cannot see Metal devices due to sandboxing/virtualization/entitlement restrictions
**Hardware:** Apple M4 Max (Metal supported, but not visible from process)

## Problem Summary

The MLX C++ library requires Metal device access for GPU operations. When Metal devices are not visible to the process, MLX crashes during array initialization:

```
NSRangeException: __NSArray0 objectAtIndex:0
→ mlx_array_from_data → Metal device 0 selection fails
```

This is an **environmental issue**, not a code bug. The hardware supports Metal, but the current process context cannot access it.

## Verification Steps

### 1. Verify Hardware Support

```bash
# Check Metal support (should show Apple M4 Max)
system_profiler SPDisplaysDataType | grep -A 5 "Metal"
```

**Expected:** "Metal: Supported"

### 2. Test Metal Device Access

Create a test Swift file to verify Metal visibility:

```bash
cat > test_metal_access.swift << 'EOF'
import Metal

print("Testing Metal device access...")
if let device = MTLCreateSystemDefaultDevice() {
    print("✅ Metal device found: \(device.name)")
} else {
    print("❌ Metal device not visible from this process")
}

let devices = MTLCopyAllDevices()
print("Total Metal devices visible: \(devices.count)")
EOF

# Run test
swiftc test_metal_access.swift -o test_metal_access
./test_metal_access
rm test_metal_access test_metal_access.swift
```

**Expected if working:** "✅ Metal device found: Apple M4 Max"
**Current problem:** "❌ Metal device not visible from this process"

### 3. Check Process Context

```bash
# Verify you're not in SSH session (Metal requires GUI session)
echo $SSH_CONNECTION
# Should be empty

# Verify Terminal.app or iTerm2 (not tmux/screen)
echo $TERM_PROGRAM
# Should be "Apple_Terminal" or "iTerm.app"

# Check if running in virtualized environment
sysctl -n machdep.cpu.brand_string
# Should show "Apple" processor, not emulated
```

## Common Causes & Fixes

### Cause 1: SSH Session

**Problem:** Metal requires GUI session with WindowServer access
**Fix:** Run tests from local Terminal.app, not SSH

```bash
# ❌ Don't run from SSH
ssh user@localhost "cargo test"

# ✅ Run from local Terminal.app
cargo test -p adapteros-lora-mlx-ffi --features mlx --lib
```

### Cause 2: tmux/screen Session

**Problem:** Terminal multiplexers may not inherit Metal entitlements
**Fix:** Exit tmux/screen and run from native terminal

```bash
# Exit tmux/screen
exit

# Run directly in Terminal.app
cargo test -p adapteros-lora-mlx-ffi --features mlx --lib
```

### Cause 3: Virtualization (Docker/VM)

**Problem:** Containers and VMs don't have Metal device passthrough
**Fix:** Run on native macOS, not in container

```bash
# ❌ Don't run in Docker
docker run rust-test cargo test

# ✅ Run on host macOS
cargo test -p adapteros-lora-mlx-ffi --features mlx --lib
```

### Cause 4: Code Signing / Entitlements

**Problem:** Rust test binary lacks Metal entitlements
**Fix:** Add entitlements to test binary (advanced)

Create `metal-entitlements.plist`:

```xml
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
    <key>com.apple.security.app-sandbox</key>
    <false/>
    <key>com.apple.security.device.metal</key>
    <true/>
</dict>
</plist>
```

Sign test binary:

```bash
# Build test binary
cargo test -p adapteros-lora-mlx-ffi --features mlx --lib --no-run

# Find test binary
TEST_BINARY=$(find target/debug/deps -name "adapteros_lora_mlx_ffi-*" -type f -perm +111 | head -1)

# Sign with entitlements
codesign --force --sign - --entitlements metal-entitlements.plist "$TEST_BINARY"

# Run signed binary
"$TEST_BINARY" --test-threads=1
```

### Cause 5: Cursor/IDE Restrictions

**Problem:** IDE integrated terminals may have restricted permissions
**Fix:** Run from native Terminal.app, not IDE terminal

```bash
# Open native Terminal.app
open -a Terminal.app

# Navigate to project
cd /Users/mln-dev/Dev/adapter-os

# Run tests
cargo test -p adapteros-lora-mlx-ffi --features mlx --lib
```

### Cause 6: XPC Services / Background Processes

**Problem:** Background services don't have GUI session access
**Fix:** Ensure tests run in foreground Terminal.app session

## Recommended Solution

**For immediate testing:**

```bash
# 1. Open native Terminal.app (not Cursor, not SSH)
open -a Terminal.app

# 2. Navigate to project
cd /Users/mln-dev/Dev/adapter-os

# 3. Verify Metal access
swift -e 'import Metal; print(MTLCreateSystemDefaultDevice() != nil ? "✅ Metal OK" : "❌ No Metal")'

# 4. If Metal OK, run tests
cargo test -p adapteros-lora-mlx-ffi --features mlx --lib -- --nocapture

# 5. Run specific attention tests
cargo test -p adapteros-lora-mlx-ffi --features mlx attention::tests --nocapture
```

## MLX Test Strategy

MLX tests have **dual-mode** operation:

1. **Stub mode** (default): Tests compile/run without Metal (CI-safe)
2. **Real mode** (`--features mlx`): Requires Metal device access

To test without Metal requirement:

```bash
# Stub tests (no Metal needed)
cargo test -p adapteros-lora-mlx-ffi --lib

# Real GPU tests (Metal required)
cargo test -p adapteros-lora-mlx-ffi --features mlx --lib
```

## Debugging Commands

```bash
# Check if running in restricted context
env | grep -E "(SSH|TERM|DISPLAY|XPC)"

# Check Metal framework loading
otool -L target/debug/deps/adapteros_lora_mlx_ffi-* | grep Metal

# Check code signature
codesign -d -vv target/debug/deps/adapteros_lora_mlx_ffi-*

# Trace Metal API calls (requires SIP disabled)
sudo dtruss -t open ./target/debug/deps/adapteros_lora_mlx_ffi-* 2>&1 | grep Metal
```

## Prevention

Add Metal device check at test startup:

```rust
#[cfg(all(test, feature = "mlx", target_os = "macos"))]
fn ensure_metal_available() {
    use metal::Device;
    if Device::system_default().is_none() {
        eprintln!("⚠️  WARNING: Metal device not visible");
        eprintln!("   Run from Terminal.app, not SSH/tmux/IDE");
        eprintln!("   See docs/MLX_METAL_DEVICE_ACCESS.md");
        panic!("Metal device required for mlx (real) tests");
    }
}
```

## Next Steps

1. **Verify environment:** Run from Terminal.app (not Cursor/SSH)
2. **Test Metal access:** `swift -e 'import Metal; print(MTLCreateSystemDefaultDevice())'`
3. **Run tests:** `cargo test -p adapteros-lora-mlx-ffi --features mlx --lib`
4. **If still failing:** Add entitlements to test binary (see Cause 4)

## References

- [Apple Metal Programming Guide](https://developer.apple.com/metal/)
- [MLX Documentation](https://ml-explore.github.io/mlx/)
- [Rust FFI Security](https://doc.rust-lang.org/nomicon/ffi.html)
- [docs/MLX_INTEGRATION.md](./MLX_INTEGRATION.md)
- [docs/MLX_TROUBLESHOOTING.md](./MLX_TROUBLESHOOTING.md)

---

**Copyright:** 2025 JKCA / James KC Auchterlonie

