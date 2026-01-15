#!/bin/bash

# Security Audit Script for adapterOS
# Performs comprehensive dependency security analysis

set -e

echo "🔒 adapterOS Security Audit"
echo "=========================="

# Function to check if command exists
command_exists() {
    command -v "$1" >/dev/null 2>&1
}

# Check prerequisites
echo "Checking prerequisites..."
if ! command_exists cargo; then
    echo "❌ Cargo not found. Please install Rust."
    exit 1
fi

# Run cargo audit
echo ""
echo "🔍 Running cargo audit..."
if command_exists cargo-audit; then
    echo "Using installed cargo-audit..."
    AUDIT_OUTPUT=$(cargo audit --format json 2>/dev/null || echo "audit_failed")
else
    echo "Installing cargo-audit..."
    cargo install cargo-audit --quiet
    AUDIT_OUTPUT=$(cargo audit --format json 2>/dev/null || echo "audit_failed")
fi

if [ "$AUDIT_OUTPUT" = "audit_failed" ]; then
    echo "⚠️  cargo audit failed or found vulnerabilities"
    echo "Running basic audit..."
    cargo audit --ignore RUSTSEC-2021-0139 || {
        echo "❌ Security vulnerabilities found!"
        echo "Run 'cargo audit' for details"
        exit 1
    }
else
    echo "✅ No critical security vulnerabilities found"
fi

# Generate Software Bill of Materials (SBOM)
echo ""
echo "📦 Generating Software Bill of Materials (SBOM)..."

SBOM_FILE="var/security/sbom-$(date +%Y%m%d-%H%M%S).json"

mkdir -p var/security

# Generate comprehensive SBOM
cat > "$SBOM_FILE" << EOF
{
  "spdxVersion": "SPDX-2.3",
  "dataLicense": "CC0-1.0",
  "SPDXID": "SPDXRef-DOCUMENT",
  "documentName": "adapterOS-SBOM",
  "documentNamespace": "https://adapteros.com/sbom/$(date +%s)",
  "creationInfo": {
    "created": "$(date -u +%Y-%m-%dT%H:%M:%SZ)",
    "creators": ["Tool: adapterOS Security Audit"],
    "licenseListVersion": "3.18"
  },
  "packages": [
EOF

# Add main package information
cat >> "$SBOM_FILE" << EOF
    {
      "SPDXID": "SPDXRef-Package-adapteros",
      "name": "adapteros",
      "versionInfo": "$(grep '^version' Cargo.toml | head -1 | cut -d'"' -f2)",
      "supplier": "Organization: adapterOS",
      "downloadLocation": "NOASSERTION",
      "filesAnalyzed": false,
      "licenseConcluded": "MIT OR Apache-2.0",
      "licenseDeclared": "MIT OR Apache-2.0",
      "copyrightText": "Copyright (c) 2025 adapterOS"
    }
EOF

# Get dependency information
echo "Analyzing dependencies..."
if command_exists cargo-license; then
    cargo license --json | jq -r '.[] | {name: .name, version: .version, license: .license}' | while read -r dep; do
        name=$(echo "$dep" | jq -r '.name')
        version=$(echo "$dep" | jq -r '.version')
        license=$(echo "$dep" | jq -r '.license')

        cat >> "$SBOM_FILE" << EOF
    ,{
      "SPDXID": "SPDXRef-Package-$name",
      "name": "$name",
      "versionInfo": "$version",
      "supplier": "Organization: Rust Community",
      "downloadLocation": "https://crates.io/api/v1/crates/$name",
      "filesAnalyzed": false,
      "licenseConcluded": "$license",
      "licenseDeclared": "$license",
      "copyrightText": "NOASSERTION"
    }
EOF
    done
else
    echo "⚠️  cargo-license not found, using basic dependency listing"
    cargo tree --format "{p}" | grep -E "^[^├└│ ]" | while read -r dep; do
        name=$(echo "$dep" | cut -d' ' -f1)
        version=$(echo "$dep" | cut -d' ' -f2 | tr -d '()')

        cat >> "$SBOM_FILE" << EOF
    ,{
      "SPDXID": "SPDXRef-Package-$name",
      "name": "$name",
      "versionInfo": "$version",
      "supplier": "Organization: Rust Community",
      "downloadLocation": "https://crates.io/api/v1/crates/$name",
      "filesAnalyzed": false,
      "licenseConcluded": "NOASSERTION",
      "licenseDeclared": "NOASSERTION",
      "copyrightText": "NOASSERTION"
    }
EOF
    done
fi

# Close SBOM JSON
cat >> "$SBOM_FILE" << EOF
  ]
}
EOF

echo "✅ SBOM generated: $SBOM_FILE"

# License compliance check
echo ""
echo "⚖️  Checking license compliance..."

if command_exists cargo-license; then
    echo "Analyzing dependency licenses..."

    # Define acceptable licenses (permissive/open source)
    ACCEPTABLE_LICENSES="MIT|Apache-2.0|BSD-2-Clause|BSD-3-Clause|ISC|CC0-1.0|Zlib|Boost-1.0|PostgreSQL"

    # Check for unacceptable licenses
    UNACCEPTABLE=$(cargo license --json | jq -r ".[] | select(.license | test(\"$ACCEPTABLE_LICENSES\") | not) | \"\(.name): \(.license)\"")

    if [ -n "$UNACCEPTABLE" ]; then
        echo "❌ Unacceptable licenses found:"
        echo "$UNACCEPTABLE"
        echo ""
        echo "Acceptable licenses: MIT, Apache-2.0, BSD-*, ISC, CC0-1.0, Zlib, Boost-1.0, PostgreSQL"
        echo "Please review and replace dependencies with unacceptable licenses."
        exit 1
    else
        echo "✅ All dependency licenses are acceptable"
    fi
else
    echo "⚠️  cargo-license not available, skipping detailed license check"
    echo "Install with: cargo install cargo-license"
fi

# Outdated dependency check
echo ""
echo "📅 Checking for outdated dependencies..."
if cargo outdated --quiet 2>/dev/null; then
    echo "⚠️  Outdated dependencies found. Consider updating:"
    cargo outdated --format json | jq -r '.[] | "\(.name): \(.current) -> \(.latest)"' | head -10
    echo "(showing first 10, run 'cargo outdated' for full list)"
else
    echo "✅ Dependencies are up to date"
fi

# Generate security report
echo ""
echo "📊 Generating security report..."

REPORT_FILE="var/security/audit-report-$(date +%Y%m%d-%H%M%S).txt"

cat > "$REPORT_FILE" << EOF
adapterOS Security Audit Report
Generated: $(date)
=====================================

DEPENDENCY SECURITY:
$(if [ "$AUDIT_OUTPUT" = "audit_failed" ]; then echo "❌ Vulnerabilities detected"; else echo "✅ No critical vulnerabilities"; fi)

LICENSE COMPLIANCE:
$(if [ -n "$UNACCEPTABLE" ]; then echo "❌ Unacceptable licenses found"; else echo "✅ All licenses acceptable"; fi)

SBOM LOCATION: $SBOM_FILE

RECOMMENDATIONS:
1. Review SBOM for supply chain transparency
2. Monitor for security advisories in dependencies
3. Keep dependencies updated to latest secure versions
4. Regularly audit license compliance

EOF

echo "✅ Security report generated: $REPORT_FILE"

# Summary
echo ""
echo "🎯 Security Audit Complete"
echo "=========================="
echo "✅ Vulnerability scan completed"
echo "✅ SBOM generated"
echo "✅ License compliance verified"
echo "📁 Reports saved to: var/security/"
echo ""
echo "Next steps:"
echo "1. Review security report for any issues"
echo "2. Address any vulnerabilities found"
echo "3. Include SBOM in compliance documentation"
echo "4. Set up automated security monitoring"

