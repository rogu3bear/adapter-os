# Version Matcher - Quick Reference

## Quick Start

```rust
use adapteros_policy::packs::{Version, VersionRange, OsvVersionRange, CpeVersionMatcher};

// Parse a version
let v = Version::parse("1.2.3")?;

// Parse a version range
let range = VersionRange::parse(">=1.0.0,<2.0.0")?;

// Check if version matches
assert!(range.matches(&v));
```

## Version Range Syntax Cheat Sheet

| Syntax | Meaning | Example | Matches |
|--------|---------|---------|---------|
| `1.2.3` | Exact | `=1.2.3` | 1.2.3 only |
| `^1.2.3` | Caret | Compatible with 1.2.3 | ≥1.2.3, <2.0.0 |
| `~1.2.3` | Tilde | Patch updates only | ≥1.2.3, <1.3.0 |
| `1.2.*` | Wildcard | Any 1.2.x version | 1.2.0 - 1.2.999 |
| `>=1.2.3` | GTE | Greater or equal | 1.2.3, 1.2.4, ... |
| `>1.2.3` | GT | Greater than | 1.2.4, 1.2.5, ... |
| `<2.0.0` | LT | Less than | ...1.9.8, 1.9.9 |
| `<=2.0.0` | LTE | Less or equal | ...1.9.9, 2.0.0 |
| `>=1.0.0,<2.0.0` | Range | Min and max | 1.0.0 - 1.999.999 |
| `*` | Any | Any version | All versions |

## Common CVE Patterns

### Single Affected Version
```rust
// CVE affects only versions < X
let range = VersionRange::parse("<3.0.0")?;
```

### Range of Affected Versions
```rust
// CVE affects versions from X to Y
let range = VersionRange::parse(">=1.0.0,<2.0.0")?;
```

### Multiple Ranges (Same CVE)
```rust
// Some CVEs affect multiple version ranges
let ranges = vec![
    VersionRange::parse(">=1.0.0,<2.0.0")?,
    VersionRange::parse(">=3.0.0,<3.1.0")?,
];

let is_vulnerable = ranges.iter().any(|r| r.matches(&version));
```

## Ecosystem-Specific Formats

### npm (Node.js)
```rust
let range = OsvVersionRange::parse("^1.2.3", "npm")?;
```

### PyPI (Python)
```rust
let range = OsvVersionRange::parse(">=1.0.0,<2.0.0", "pypi")?;
```

### Cargo (Rust)
```rust
let range = OsvVersionRange::parse("^1.2.3", "crates.io")?;
```

### Maven (Java)
```rust
// Maven format: [min,max] or (min,max)
let range = OsvVersionRange::parse("[1.0.0,2.0.0]", "maven")?;
```

### NVD CPE Format
```rust
// Format: part:vendor:product:version_constraint
let cpe = CpeVersionMatcher::parse("a:apache:log4j:>=1.0.0,<2.0.0")?;
```

## Version Parsing

### Supported Formats
```rust
Version::parse("1.2.3")?                    // Basic semver
Version::parse("v1.2.3")?                   // With v prefix
Version::parse("1.2.3-alpha")?              // Pre-release
Version::parse("1.2.3-rc.1")?               // Pre-release with number
Version::parse("1.2.3+build.123")?          // Build metadata
Version::parse("1.2.3-rc.1+build.123")?     // Full format
```

## Fuzzy Matching

```rust
let range = VersionRange::parse("=1.2.3")?;

// Exact match (tolerance = 0)
range.matches_fuzzy(&Version::parse("1.2.3")?, 0)  // ✓

// Patch tolerance = 1
range.matches_fuzzy(&Version::parse("1.2.4")?, 1)  // ✓
range.matches_fuzzy(&Version::parse("1.2.5")?, 1)  // ✗

// Patch tolerance = 2
range.matches_fuzzy(&Version::parse("1.2.5")?, 2)  // ✓
```

## Min/Max Version Extraction

```rust
let range = VersionRange::parse(">=1.0.0,<2.0.0")?;

if let Some(min) = range.min_version() {
    println!("Minimum affected: {}", min); // 1.0.0
}

if let Some(max) = range.max_version() {
    println!("Maximum affected: {}", max); // 2.0.0
}
```

## Real CVE Examples

### Log4j (CVE-2021-44228)
```rust
let range = VersionRange::parse(">=2.0.0,<2.16.0")?;
assert!(range.matches(&Version::parse("2.13.0")?)); // Vulnerable
assert!(!range.matches(&Version::parse("2.16.0")?)); // Fixed
```

### Spring Framework (CVE-2022-22965)
```rust
let range1 = VersionRange::parse(">=3.2.0,<5.2.25")?;
let range2 = VersionRange::parse(">=5.3.0,<5.3.14")?;

let v = Version::parse("5.3.13")?;
assert!(range2.matches(&v)); // Vulnerable in range 2
```

### curl (CVE-2023-38545)
```rust
let range = VersionRange::parse("<8.0.0")?;
assert!(range.matches(&Version::parse("7.99.9")?)); // Vulnerable
assert!(!range.matches(&Version::parse("8.0.0")?)); // Fixed
```

## Error Handling

```rust
use adapteros_core::AosError;

match Version::parse("invalid") {
    Ok(_) => println!("Valid version"),
    Err(AosError::Validation(msg)) => eprintln!("Invalid: {}", msg),
    Err(e) => eprintln!("Error: {:?}", e),
}
```

## Testing

### Run all tests
```bash
cargo test -p adapteros-policy version_matcher
```

### Run integration tests
```bash
cargo test -p adapteros-policy --test version_matcher_tests
```

### Run specific test
```bash
cargo test -p adapteros-policy version_matcher::tests::test_real_world_log4j_cve
```

## Key Types

| Type | Purpose |
|------|---------|
| `Version` | Represents a semantic version |
| `VersionRange` | Represents a version constraint |
| `OsvVersionRange` | Handles OSV database formats |
| `CpeVersionMatcher` | Handles NVD CPE formats |

## Common Operations

```rust
// Check if vulnerable
if range.matches(&version) {
    println!("VULNERABLE!");
}

// Get affected range
if let Some(min) = range.min_version() {
    println!("From: {}", min);
}

// Display range as string
println!("Range: {}", range);

// Parse various formats
VersionRange::parse("^1.2.3")?      // SemVer
VersionRange::parse("1.2.*")?       // Wildcard
VersionRange::parse(">=1.0.0,<2")?  // Compound
```

## Caret (^) Rules

- `^1.2.3` → `>=1.2.3, <2.0.0`
- `^0.2.3` → `>=0.2.3, <0.3.0` (special case: 0.x versions)
- `^0.0.3` → `>=0.0.3, <0.0.4` (special case: 0.0.x versions)

## Tilde (~) Rules

- `~1.2.3` → `>=1.2.3, <1.3.0`
- `~1.2` → `>=1.2.0, <1.3.0`
- `~1` → `>=1.0.0, <2.0.0`

## Integration Points

### With Dependency Security Policy
```rust
// In dependency_security.rs
let range = VersionRange::parse(&cve_range_string)?;
if range.matches(&current_version) {
    // Flag as vulnerable
}
```

### With CVE Clients
```rust
// NVD client integration
let cpe = CpeVersionMatcher::parse(cpe_string)?;
if cpe.matches(&app_version) {
    // Log vulnerability
}

// OSV client integration
let osv = OsvVersionRange::parse(range_str, ecosystem)?;
if osv.matches(&app_version) {
    // Report vulnerability
}
```

## Performance Tips

- Parse versions/ranges once, reuse for multiple matches: `O(1)` per match
- Batch match against multiple ranges: `O(r)` where r = number of ranges
- Pre-compile frequently used ranges
- Avoid parsing in tight loops

## Troubleshooting

### Invalid version string
```
AosError::Validation("Invalid version format: ...")
```
Check format is `MAJOR.MINOR.PATCH[-PRERELEASE][+BUILD]`

### Invalid range syntax
```
AosError::Validation("Invalid compound range: ...")
```
Check syntax like `>=1.0.0,<2.0.0` (comma-separated, two constraints)

### Unexpected ecosystem format
Uses generic fallback parsing if ecosystem not recognized
