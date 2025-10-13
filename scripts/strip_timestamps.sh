#!/bin/bash
# Strip timestamps from binaries for reproducibility

set -e

echo "🔧 Stripping timestamps from binaries..."

# Function to strip timestamps from a binary
strip_timestamps() {
    local binary="$1"
    if [ ! -f "$binary" ]; then
        echo "⚠️ Binary not found: $binary"
        return 1
    fi
    
    echo "   Processing: $binary"
    
    # Strip debug symbols and timestamps, but preserve UUID load commands
    # Use -x to remove local symbols but keep UUID for macOS compatibility
    strip -x "$binary" 2>/dev/null || true
    
    # Remove .DS_Store if present
    if [ -f "${binary}.DS_Store" ]; then
        rm -f "${binary}.DS_Store"
    fi
    
    # Normalize file permissions
    chmod 755 "$binary"
    
    # Touch with SOURCE_DATE_EPOCH if set
    if [ -n "$SOURCE_DATE_EPOCH" ]; then
        touch -t "$(date -r "$SOURCE_DATE_EPOCH" +%Y%m%d%H%M.%S)" "$binary"
    else
        # Use fixed timestamp for reproducibility
        touch -t "202401010000.00" "$binary"
    fi
    
    echo "   ✅ Stripped: $binary"
}

# Find all executables in target/release
if [ -d "target/release" ]; then
    echo "📁 Processing target/release binaries..."
    
    # Process main binaries
    for binary in target/release/aosctl target/release/adapteros-server; do
        if [ -f "$binary" ]; then
            strip_timestamps "$binary"
        fi
    done
    
    # Process any other executables
    find target/release -type f -executable -name "*.exe" -o -name "aosctl" -o -name "adapteros-server" | while read -r binary; do
        strip_timestamps "$binary"
    done
else
    echo "⚠️ target/release directory not found"
fi

# Process Metal shaders
if [ -d "metal" ]; then
    echo "📁 Processing Metal shaders..."
    
    for metallib in metal/*.metallib; do
        if [ -f "$metallib" ]; then
            echo "   Processing: $metallib"
            
            # Normalize file permissions
            chmod 644 "$metallib"
            
            # Touch with SOURCE_DATE_EPOCH if set
            if [ -n "$SOURCE_DATE_EPOCH" ]; then
                touch -t "$(date -r "$SOURCE_DATE_EPOCH" +%Y%m%d%H%M.%S)" "$metallib"
            else
                touch -t "202401010000.00" "$metallib"
            fi
            
            echo "   ✅ Normalized: $metallib"
        fi
    done
fi

# Process SBOM files
if [ -f "target/sbom.spdx.json" ]; then
    echo "📁 Processing SBOM files..."
    
    # Normalize file permissions
    chmod 644 target/sbom.spdx.json
    
    # Touch with SOURCE_DATE_EPOCH if set
    if [ -n "$SOURCE_DATE_EPOCH" ]; then
        touch -t "$(date -r "$SOURCE_DATE_EPOCH" +%Y%m%d%H%M.%S)" target/sbom.spdx.json
    else
        touch -t "202401010000.00" target/sbom.spdx.json
    fi
    
    echo "   ✅ Normalized: target/sbom.spdx.json"
fi

# Remove .DS_Store files
echo "🧹 Cleaning .DS_Store files..."
find . -name ".DS_Store" -delete 2>/dev/null || true

# Remove other macOS metadata
echo "🧹 Cleaning macOS metadata..."
find . -name "._*" -delete 2>/dev/null || true

echo "✅ Timestamp stripping complete"
echo "   All binaries normalized for reproducibility"
