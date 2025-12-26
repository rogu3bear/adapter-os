# Version Matcher Module

## Overview

The `version_matcher` module provides comprehensive version range matching capabilities for CVE (Common Vulnerabilities and Exposures) integration in AdapterOS. It enables matching software versions against vulnerability databases including NVD (National Vulnerability Database), OSV (Open Source Vulnerabilities), and CPE (Common Platform Enumeration) formats.

## Key Features

- **Semantic Versioning (SemVer)**: Full support for major.minor.patch versions with pre-release and build metadata
- **Multiple Range Syntax**: Caret (^), tilde (~), wildcard (*), and compound ranges
- **Cargo Compatibility**: Native support for Rust/Cargo version syntax
- **Ecosystem Support**: Handles ecosystem-specific formats (npm, PyPI, Maven, NuGet, crates.io)
- **CPE Format**: Supports CPE version strings for NVD database lookups
- **Fuzzy Matching**: Allows patch-level tolerance matching
- **Real-world CVE Coverage**: Tested against actual CVE vulnerability ranges

## Module Structure

### Core Types

#### `Version`
Represents a semantic version with components:
- `major: u32` - Major version number
- `minor: u32` - Minor version number
- `patch: u32` - Patch version number
- `pre_release: Option<String>` - Pre-release identifier (e.g., "alpha.1")
- `build: Option<String>` - Build metadata (e.g., "build.123")

**Methods:**
- `parse(s: &str) -> Result<Self>` - Parse version from string
- `compare_core(&self, other: &Self) -> Ordering` - Compare core versions
- `as_tuple(&self) -> (u32, u32, u32)` - Get version as tuple
- `is_wildcard(&self) -> bool` - Check if wildcard version

**Supported Formats:**
```rust
Version::parse("1.2.3")?               // Basic semver
Version::parse("v1.2.3")?              // With v prefix
Version::parse("1.2.3-alpha.1")?       // Pre-release
Version::parse("1.2.3+build.123")?     // Build metadata
Version::parse("1.2.3-rc.1+build.123")? // Full format
```

#### `VersionRange`
Represents version range constraints for CVE matching:

**Enum Variants:**
- `Exact(Version)` - Exact version match
- `Range { min, max, min_inclusive, max_inclusive }` - Bounded range with explicit inclusivity
- `GreaterOrEqual(Version)` - >= constraint
- `GreaterThan(Version)` - > constraint
- `LessOrEqual(Version)` - <= constraint
- `LessThan(Version)` - < constraint
- `Caret(Version)` - ^ compatible versions
- `Tilde(Version)` - ~ reasonably close versions
- `Wildcard(major, minor)` - X.Y.* format
- `Any` - Matches any version

**Methods:**
- `parse(s: &str) -> Result<Self>` - Parse range from string
- `matches(&self, version: &Version) -> bool` - Check if version matches
- `matches_fuzzy(&self, version: &Version, patch_tolerance: u32) -> bool` - Fuzzy match
- `min_version(&self) -> Option<&Version>` - Get minimum version
- `max_version(&self) -> Option<&Version>` - Get maximum version

**Supported Syntax:**
```rust
VersionRange::parse("=1.2.3")?              // Exact match
VersionRange::parse("1.2.3")?               // Exact (implicit)
VersionRange::parse("^1.2.3")?              // Caret (SemVer compatible)
VersionRange::parse("~1.2.3")?              // Tilde (SemVer compatible)
VersionRange::parse("1.2.*")?               // Wildcard
VersionRange::parse(">1.2.3")?              // Greater than
VersionRange::parse(">=1.2.3")?             // Greater or equal
VersionRange::parse("<2.0.0")?              // Less than
VersionRange::parse("<=2.0.0")?             // Less or equal
VersionRange::parse(">=1.0.0,<2.0.0")?     // Compound range
VersionRange::parse("*")?                   // Any version
```

**Serialization Compatibility:**
- New serialization keeps inclusivity flags: `Range { min, max, min_inclusive, max_inclusive }`.
- Legacy payloads serialized as tuples (e.g., `"Range":[min,max]`) still deserialize and default to `min_inclusive=true`, `max_inclusive=false`.
- When persisting or sending `VersionRange`, prefer the flag-aware shape; legacy readers remain compatible via the tuple fallback.

#### `OsvVersionRange`
Handles ecosystem-specific version ranges from OSV database:

**Properties:**
- `affected: String` - Original affected version string
- `ecosystem: String` - Ecosystem name (npm, pypi, crates.io, maven, nuget)
- `constraint: VersionRange` - Parsed constraint

**Methods:**
- `parse(affected: &str, ecosystem: &str) -> Result<Self>` - Parse OSV range
- `matches(&self, version: &Version) -> bool` - Check match

**Ecosystem Support:**
- `npm` - SemVer ranges (^, ~, ranges)
- `crates.io` / `cargo` - Cargo semver syntax
- `pypi` - PEP 440 version specifiers
- `maven` - Maven version ranges
- `nuget` - NuGet version syntax

#### `CpeVersionMatcher`
Handles CPE (Common Platform Enumeration) format matching for NVD:

**Properties:**
- `part: String` - CPE part (a=application, o=OS, h=hardware)
- `vendor: String` - Vendor name
- `product: String` - Product name
- `version_constraint: VersionRange` - Version constraint

**Methods:**
- `parse(cpe: &str) -> Result<Self>` - Parse CPE string
- `matches(&self, version: &Version) -> bool` - Check match
- `matches_string(&self, version_str: &str) -> Result<bool>` - Match string version

**CPE Format:**
```rust
CpeVersionMatcher::parse("a:apache:log4j:>=1.0.0,<2.0.0")?
// part="a", vendor="apache", product="log4j"
```

## Usage Examples

### Basic Version Matching

```rust
use adapteros_policy::packs::{Version, VersionRange};

// Check if version matches a specific CVE range
let cve_range = VersionRange::parse(">=2.0.0,<2.16.0")?;
let version = Version::parse("2.13.0")?;

if cve_range.matches(&version) {
    println!("Version is vulnerable!");
}
```

### Caret Range Matching

```rust
// ^1.2.3 = >=1.2.3, <2.0.0 (SemVer compatible)
let range = VersionRange::parse("^1.2.3")?;

assert!(range.matches(&Version::parse("1.2.3")?));  // ✓
assert!(range.matches(&Version::parse("1.5.0")?));  // ✓
assert!(!range.matches(&Version::parse("2.0.0")?)); // ✗
```

### Tilde Range Matching

```rust
// ~1.2.3 = >=1.2.3, <1.3.0 (reasonably close)
let range = VersionRange::parse("~1.2.3")?;

assert!(range.matches(&Version::parse("1.2.3")?));  // ✓
assert!(range.matches(&Version::parse("1.2.99")?)); // ✓
assert!(!range.matches(&Version::parse("1.3.0")?)); // ✗
```

### Fuzzy Matching

```rust
// Allow 2-patch level tolerance
let range = VersionRange::parse("=1.2.3")?;
let version = Version::parse("1.2.5")?;

assert!(range.matches_fuzzy(&version, 2)); // ✓ (within 2 patches)
assert!(!range.matches_fuzzy(&version, 1)); // ✗ (exceed 1 patch tolerance)
```

### OSV Database Integration

```rust
use adapteros_policy::packs::OsvVersionRange;

// npm ecosystem
let npm_range = OsvVersionRange::parse("^1.2.3", "npm")?;
let vulnerable = Version::parse("1.5.0")?;
assert!(npm_range.matches(&vulnerable));

// PyPI ecosystem
let pypi_range = OsvVersionRange::parse(">=1.0.0,<2.0.0", "pypi")?;
let safe = Version::parse("2.0.0")?;
assert!(!pypi_range.matches(&safe));
```

### CPE Format Integration

```rust
use adapteros_policy::packs::CpeVersionMatcher;

// NVD CPE format
let cpe = CpeVersionMatcher::parse("a:apache:log4j:>=1.0.0,<2.0.0")?;

assert!(cpe.matches_string("1.5.0")?);    // ✓
assert!(!cpe.matches_string("2.0.0")?);   // ✗
```

### Multiple Vulnerable Ranges

```rust
// Check version against multiple affected ranges (typical CVE scenario)
let ranges = vec![
    VersionRange::parse(">=1.0.0,<2.0.0")?,
    VersionRange::parse(">=3.0.0,<3.1.0")?,
];

let version = Version::parse("3.0.5")?;
let is_vulnerable = ranges.iter().any(|r| r.matches(&version));
```

## Real-World CVE Examples

### CVE-2021-44228 (Apache Log4j)
```rust
// Vulnerable: 2.0-beta9 through 2.15.0
let range = VersionRange::parse(">=2.0.0,<2.16.0")?;

// Vulnerable versions
assert!(range.matches(&Version::parse("2.8.1")?));
assert!(range.matches(&Version::parse("2.13.0")?));
assert!(range.matches(&Version::parse("2.15.0")?));

// Fixed versions
assert!(!range.matches(&Version::parse("2.16.0")?));
assert!(!range.matches(&Version::parse("2.17.0")?));
```

### CVE-2022-22965 (Spring Framework RCE)
```rust
// Multiple affected ranges
let range1 = VersionRange::parse(">=3.2.0,<5.2.25")?;
let range2 = VersionRange::parse(">=5.3.0,<5.3.14")?;

// Vulnerable
assert!(range1.matches(&Version::parse("5.2.24")?));
assert!(range2.matches(&Version::parse("5.3.13")?));

// Fixed
assert!(!range1.matches(&Version::parse("5.2.25")?));
assert!(!range2.matches(&Version::parse("5.3.14")?));
```

### CVE-2023-38545 (curl vulnerability)
```rust
// All versions < 8.0.0
let range = VersionRange::parse("<8.0.0")?;

assert!(range.matches(&Version::parse("7.99.9")?));
assert!(!range.matches(&Version::parse("8.0.0")?));
```

## Semver Compatibility Rules

### Caret (^) Ranges
The caret allows changes that don't modify the left-most non-zero element:

```
^1.2.3  := >=1.2.3, <2.0.0   (left-most non-zero is major)
^0.2.3  := >=0.2.3, <0.3.0   (left-most non-zero is minor)
^0.0.3  := >=0.0.3, <0.0.4   (left-most non-zero is patch)
```

### Tilde (~) Ranges
The tilde allows patch-level changes:

```
~1.2.3  := >=1.2.3, <1.3.0   (allows patch changes)
~1.2    := >=1.2.0, <1.3.0   (allows patch changes)
~1      := >=1.0.0, <2.0.0   (allows minor changes)
```

## Error Handling

```rust
use adapteros_core::AosError;

match Version::parse("invalid.version") {
    Ok(v) => println!("Parsed: {}", v),
    Err(AosError::Validation(msg)) => eprintln!("Invalid version: {}", msg),
    Err(e) => eprintln!("Error: {:?}", e),
}

match VersionRange::parse(">=1.2.3,>2.0.0") {
    Ok(r) => println!("Range: {}", r),
    Err(AosError::Validation(msg)) => eprintln!("Invalid range: {}", msg),
    Err(e) => eprintln!("Error: {:?}", e),
}
```

## Integration with CVE Databases

### NVD Integration
```rust
// NVD returns CPE version ranges
let cpe_str = "a:vendor:product:version_spec";
let cpe = CpeVersionMatcher::parse(cpe_str)?;

if cpe.matches(&current_version) {
    // Version is vulnerable according to NVD
}
```

### OSV Integration
```rust
// OSV returns ecosystem-specific ranges
let osv = OsvVersionRange::parse(">=1.0.0,<2.0.0", "npm")?;

if osv.matches(&current_version) {
    // Version is vulnerable according to OSV
}
```

## Performance Considerations

- Version parsing is O(n) where n is string length
- Version comparison is O(1) for core versions
- Range matching is O(1) for most range types
- Pre-release comparison may involve string comparison
- Batch matching multiple ranges is O(r*1) where r is number of ranges

## Testing

The module includes comprehensive test coverage:

- **Unit tests** in the module itself (800+ lines of tests)
- **Integration tests** in `tests/version_matcher_tests.rs`
- **Real-world CVE test cases** based on actual vulnerabilities
- **Fuzzy matching tests** for patch tolerance
- **Ecosystem-specific tests** for npm, PyPI, Maven, etc.

Run tests with:
```bash
cargo test -p adapteros-policy version_matcher
cargo test -p adapteros-policy --test version_matcher_tests
```

## Future Enhancements

- Support for version constraints with pre-release precedence
- Pre-release version range matching
- Performance optimization for large batch operations
- Caching layer for frequently matched versions
- Additional ecosystem support (Go, Ruby gems, PHP)
- Version range normalization and simplification
- Range intersection and union operations

## References

- [Semantic Versioning 2.0.0](https://semver.org/)
- [Cargo Version Requirements](https://doc.rust-lang.org/cargo/mastering/publishing.html#the-publish-field)
- [OSV Database Format](https://github.com/google/osv)
- [CPE Common Platform Enumeration](https://nvlpubs.nist.gov/nistpubs/Legacy/SP/nistspecialpublication800-188.pdf)
- [PEP 440 - Version Identification](https://www.python.org/dev/peps/pep-0440/)
