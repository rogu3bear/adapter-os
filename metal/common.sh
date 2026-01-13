#!/bin/bash
# Shared Metal toolchain detection logic
# Used by build scripts to reliably find Metal toolchain binaries

# Resolve Metal toolchain binary path
# Returns path to metal binary if found, empty otherwise
resolve_metal_toolchain() {
    # Check METAL_TOOLCHAIN_BIN env var
    if [ -n "${METAL_TOOLCHAIN_BIN:-}" ] && [ -f "${METAL_TOOLCHAIN_BIN}/metal" ]; then
        echo "${METAL_TOOLCHAIN_BIN}/metal"
        return 0
    fi
    
    # Check ~/Library/Developer/Toolchains/Metal.xctoolchain/usr/bin
    if [ -f "${HOME}/Library/Developer/Toolchains/Metal.xctoolchain/usr/bin/metal" ]; then
        echo "${HOME}/Library/Developer/Toolchains/Metal.xctoolchain/usr/bin/metal"
        return 0
    fi
    
    # Check /Library/Developer/Toolchains/Metal.xctoolchain/usr/bin
    if [ -f "/Library/Developer/Toolchains/Metal.xctoolchain/usr/bin/metal" ]; then
        echo "/Library/Developer/Toolchains/Metal.xctoolchain/usr/bin/metal"
        return 0
    fi
    
    # Check /Applications/Xcode.app/Contents/Developer/Toolchains/Metal.xctoolchain/usr/bin
    if [ -f "/Applications/Xcode.app/Contents/Developer/Toolchains/Metal.xctoolchain/usr/bin/metal" ]; then
        echo "/Applications/Xcode.app/Contents/Developer/Toolchains/Metal.xctoolchain/usr/bin/metal"
        return 0
    fi
    
    return 1
}

# Resolve metallib binary path
resolve_metallib_toolchain() {
    local metal_path=$(resolve_metal_toolchain)
    if [ -n "$metal_path" ]; then
        echo "${metal_path%/*}/metallib"
        return 0
    fi
    return 1
}

# Get SDK path for Metal toolchain binaries
get_sdk_path() {
    xcrun --show-sdk-path 2>/dev/null || echo ""
}
