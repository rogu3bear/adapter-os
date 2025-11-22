//! Integration tests for version matcher module
//!
//! Tests comprehensive version range matching functionality for CVE integration

use adapteros_policy::packs::{CpeVersionMatcher, OsvVersionRange, Version, VersionRange};

#[test]
fn test_version_parse_basic() {
    let v = Version::parse("1.2.3").unwrap();
    assert_eq!(v.major, 1);
    assert_eq!(v.minor, 2);
    assert_eq!(v.patch, 3);
    assert!(v.pre_release.is_none());
    assert!(v.build.is_none());
}

#[test]
fn test_version_parse_with_v_prefix() {
    let v = Version::parse("v1.2.3").unwrap();
    assert_eq!(v.major, 1);
    assert_eq!(v.minor, 2);
    assert_eq!(v.patch, 3);
}

#[test]
fn test_version_parse_prerelease() {
    let v = Version::parse("1.2.3-alpha.1").unwrap();
    assert_eq!(v.major, 1);
    assert_eq!(v.minor, 2);
    assert_eq!(v.patch, 3);
    assert_eq!(v.pre_release, Some("alpha.1".to_string()));
}

#[test]
fn test_version_parse_with_build() {
    let v = Version::parse("1.2.3+build.123").unwrap();
    assert_eq!(v.major, 1);
    assert_eq!(v.minor, 2);
    assert_eq!(v.patch, 3);
    assert_eq!(v.build, Some("build.123".to_string()));
}

#[test]
fn test_version_parse_full() {
    let v = Version::parse("1.2.3-rc.1+build.123").unwrap();
    assert_eq!(v.major, 1);
    assert_eq!(v.minor, 2);
    assert_eq!(v.patch, 3);
    assert_eq!(v.pre_release, Some("rc.1".to_string()));
    assert_eq!(v.build, Some("build.123".to_string()));
}

#[test]
fn test_version_comparison() {
    let v1 = Version::parse("1.2.3").unwrap();
    let v2 = Version::parse("1.2.4").unwrap();
    let v3 = Version::parse("2.0.0").unwrap();

    assert!(v1 < v2);
    assert!(v2 < v3);
    assert!(v1 < v3);
}

#[test]
fn test_version_prerelease_ordering() {
    let stable = Version::parse("1.2.3").unwrap();
    let prerelease = Version::parse("1.2.3-alpha").unwrap();
    assert!(prerelease < stable);
}

#[test]
fn test_version_range_exact() {
    let range = VersionRange::parse("=1.2.3").unwrap();
    let v1 = Version::parse("1.2.3").unwrap();
    let v2 = Version::parse("1.2.4").unwrap();

    assert!(range.matches(&v1));
    assert!(!range.matches(&v2));
}

#[test]
fn test_version_range_exact_implicit() {
    let range = VersionRange::parse("1.2.3").unwrap();
    let v1 = Version::parse("1.2.3").unwrap();
    let v2 = Version::parse("1.2.4").unwrap();

    assert!(range.matches(&v1));
    assert!(!range.matches(&v2));
}

#[test]
fn test_version_range_caret() {
    let range = VersionRange::parse("^1.2.3").unwrap();
    let v1 = Version::parse("1.2.3").unwrap();
    let v2 = Version::parse("1.2.4").unwrap();
    let v3 = Version::parse("1.5.0").unwrap();
    let v4 = Version::parse("2.0.0").unwrap();

    assert!(range.matches(&v1));
    assert!(range.matches(&v2));
    assert!(range.matches(&v3));
    assert!(!range.matches(&v4));
}

#[test]
fn test_version_range_caret_zero_major() {
    let range = VersionRange::parse("^0.2.3").unwrap();
    let v1 = Version::parse("0.2.3").unwrap();
    let v2 = Version::parse("0.2.4").unwrap();
    let v3 = Version::parse("0.3.0").unwrap();

    assert!(range.matches(&v1));
    assert!(range.matches(&v2));
    assert!(!range.matches(&v3));
}

#[test]
fn test_version_range_tilde() {
    let range = VersionRange::parse("~1.2.3").unwrap();
    let v1 = Version::parse("1.2.3").unwrap();
    let v2 = Version::parse("1.2.4").unwrap();
    let v3 = Version::parse("1.3.0").unwrap();

    assert!(range.matches(&v1));
    assert!(range.matches(&v2));
    assert!(!range.matches(&v3));
}

#[test]
fn test_version_range_greater_or_equal() {
    let range = VersionRange::parse(">=1.2.3").unwrap();
    let v1 = Version::parse("1.2.2").unwrap();
    let v2 = Version::parse("1.2.3").unwrap();
    let v3 = Version::parse("1.2.4").unwrap();

    assert!(!range.matches(&v1));
    assert!(range.matches(&v2));
    assert!(range.matches(&v3));
}

#[test]
fn test_version_range_greater_than() {
    let range = VersionRange::parse(">1.2.3").unwrap();
    let v1 = Version::parse("1.2.3").unwrap();
    let v2 = Version::parse("1.2.4").unwrap();

    assert!(!range.matches(&v1));
    assert!(range.matches(&v2));
}

#[test]
fn test_version_range_less_than() {
    let range = VersionRange::parse("<2.0.0").unwrap();
    let v1 = Version::parse("1.9.9").unwrap();
    let v2 = Version::parse("2.0.0").unwrap();

    assert!(range.matches(&v1));
    assert!(!range.matches(&v2));
}

#[test]
fn test_version_range_less_or_equal() {
    let range = VersionRange::parse("<=2.0.0").unwrap();
    let v1 = Version::parse("1.9.9").unwrap();
    let v2 = Version::parse("2.0.0").unwrap();
    let v3 = Version::parse("2.0.1").unwrap();

    assert!(range.matches(&v1));
    assert!(range.matches(&v2));
    assert!(!range.matches(&v3));
}

#[test]
fn test_version_range_wildcard() {
    let range = VersionRange::parse("1.2.*").unwrap();
    let v1 = Version::parse("1.2.0").unwrap();
    let v2 = Version::parse("1.2.99").unwrap();
    let v3 = Version::parse("1.3.0").unwrap();

    assert!(range.matches(&v1));
    assert!(range.matches(&v2));
    assert!(!range.matches(&v3));
}

#[test]
fn test_version_range_compound() {
    let range = VersionRange::parse(">=1.0.0,<2.0.0").unwrap();
    let v1 = Version::parse("0.9.9").unwrap();
    let v2 = Version::parse("1.0.0").unwrap();
    let v3 = Version::parse("1.5.0").unwrap();
    let v4 = Version::parse("2.0.0").unwrap();

    assert!(!range.matches(&v1));
    assert!(range.matches(&v2));
    assert!(range.matches(&v3));
    assert!(!range.matches(&v4));
}

#[test]
fn test_version_range_any() {
    let range = VersionRange::parse("*").unwrap();
    let v1 = Version::parse("0.0.1").unwrap();
    let v2 = Version::parse("1.2.3").unwrap();
    let v3 = Version::parse("999.999.999").unwrap();

    assert!(range.matches(&v1));
    assert!(range.matches(&v2));
    assert!(range.matches(&v3));
}

#[test]
fn test_version_fuzzy_matching() {
    let range = VersionRange::parse("=1.2.3").unwrap();
    let v1 = Version::parse("1.2.3").unwrap();
    let v2 = Version::parse("1.2.4").unwrap();
    let v3 = Version::parse("1.2.5").unwrap();

    assert!(range.matches_fuzzy(&v1, 0));
    assert!(range.matches_fuzzy(&v2, 1));
    assert!(!range.matches_fuzzy(&v2, 0));
    assert!(range.matches_fuzzy(&v3, 2));
    assert!(!range.matches_fuzzy(&v3, 1));
}

#[test]
fn test_osv_version_range_npm() {
    let osv = OsvVersionRange::parse("^1.2.3", "npm").unwrap();
    let v1 = Version::parse("1.2.3").unwrap();
    let v2 = Version::parse("1.5.0").unwrap();
    let v3 = Version::parse("2.0.0").unwrap();

    assert!(osv.matches(&v1));
    assert!(osv.matches(&v2));
    assert!(!osv.matches(&v3));
}

#[test]
fn test_osv_version_range_crates() {
    let osv = OsvVersionRange::parse(">=1.0.0,<2.0.0", "crates.io").unwrap();
    let v1 = Version::parse("1.0.0").unwrap();
    let v2 = Version::parse("1.5.0").unwrap();
    let v3 = Version::parse("2.0.0").unwrap();

    assert!(osv.matches(&v1));
    assert!(osv.matches(&v2));
    assert!(!osv.matches(&v3));
}

#[test]
fn test_osv_version_range_pypi() {
    let osv = OsvVersionRange::parse(">=1.0.0,<2.0.0", "pypi").unwrap();
    let v1 = Version::parse("1.0.0").unwrap();
    let v2 = Version::parse("1.5.0").unwrap();
    let v3 = Version::parse("2.0.0").unwrap();

    assert!(osv.matches(&v1));
    assert!(osv.matches(&v2));
    assert!(!osv.matches(&v3));
}

#[test]
fn test_cpe_version_matcher_parse() {
    let cpe = CpeVersionMatcher::parse("a:apache:log4j:>=1.0.0,<2.0.0").unwrap();
    assert_eq!(cpe.part, "a");
    assert_eq!(cpe.vendor, "apache");
    assert_eq!(cpe.product, "log4j");
}

#[test]
fn test_cpe_version_matcher_matches() {
    let cpe = CpeVersionMatcher::parse("a:apache:log4j:>=1.0.0,<2.0.0").unwrap();
    assert!(cpe.matches_string("1.0.0").unwrap());
    assert!(cpe.matches_string("1.5.0").unwrap());
    assert!(!cpe.matches_string("2.0.0").unwrap());
}

#[test]
fn test_version_range_display() {
    assert_eq!(
        format!("{}", VersionRange::parse("=1.2.3").unwrap()),
        "=1.2.3"
    );
    assert_eq!(
        format!("{}", VersionRange::parse("^1.2.3").unwrap()),
        "^1.2.3"
    );
    assert_eq!(
        format!("{}", VersionRange::parse("~1.2.3").unwrap()),
        "~1.2.3"
    );
    assert_eq!(
        format!("{}", VersionRange::parse("1.2.*").unwrap()),
        "1.2.*"
    );
    assert_eq!(format!("{}", VersionRange::parse("*").unwrap()), "*");
}

// Real-world CVE test cases
#[test]
fn test_real_world_log4j_cve() {
    // CVE-2021-44228: Apache Log4j 2.0-beta9 through 2.15.0
    let range = VersionRange::parse(">=2.0.0,<2.16.0").unwrap();
    let vulnerable_versions = vec!["2.0.0", "2.8.1", "2.13.0", "2.14.0", "2.15.0"];
    let safe_versions = vec!["1.2.17", "2.16.0", "2.16.1", "3.0.0"];

    for v_str in vulnerable_versions {
        let v = Version::parse(v_str).unwrap();
        assert!(range.matches(&v), "Expected {} to be vulnerable", v_str);
    }

    for v_str in safe_versions {
        let v = Version::parse(v_str).unwrap();
        assert!(!range.matches(&v), "Expected {} to be safe", v_str);
    }
}

#[test]
fn test_real_world_spring_rce() {
    // CVE-2022-22965: Spring Framework >=3.2.0, <5.2.25, >=5.3.0, <5.3.14
    let range1 = VersionRange::parse(">=3.2.0,<5.2.25").unwrap();
    let range2 = VersionRange::parse(">=5.3.0,<5.3.14").unwrap();

    let v1 = Version::parse("3.2.0").unwrap();
    let v2 = Version::parse("5.2.24").unwrap();
    let v3 = Version::parse("5.3.0").unwrap();
    let v4 = Version::parse("5.3.13").unwrap();
    let v5 = Version::parse("5.2.25").unwrap();
    let v6 = Version::parse("5.3.14").unwrap();

    assert!(range1.matches(&v1));
    assert!(range1.matches(&v2));
    assert!(!range1.matches(&v5));

    assert!(range2.matches(&v3));
    assert!(range2.matches(&v4));
    assert!(!range2.matches(&v6));
}

#[test]
fn test_real_world_curl_cve() {
    // CVE-2023-38545: curl <8.0.0
    let range = VersionRange::parse("<8.0.0").unwrap();
    let vulnerable = Version::parse("7.99.9").unwrap();
    let safe = Version::parse("8.0.0").unwrap();

    assert!(range.matches(&vulnerable));
    assert!(!range.matches(&safe));
}

#[test]
fn test_cargo_syntax_exact() {
    let range = VersionRange::parse("1.2.3").unwrap();
    let v = Version::parse("1.2.3").unwrap();
    assert!(range.matches(&v));
}

#[test]
fn test_cargo_syntax_caret() {
    let range = VersionRange::parse("^1.2.3").unwrap();
    let v1 = Version::parse("1.2.3").unwrap();
    let v2 = Version::parse("1.9.0").unwrap();
    let v3 = Version::parse("2.0.0").unwrap();

    assert!(range.matches(&v1));
    assert!(range.matches(&v2));
    assert!(!range.matches(&v3));
}

#[test]
fn test_cargo_syntax_tilde() {
    let range = VersionRange::parse("~1.2.3").unwrap();
    let v1 = Version::parse("1.2.3").unwrap();
    let v2 = Version::parse("1.2.99").unwrap();
    let v3 = Version::parse("1.3.0").unwrap();

    assert!(range.matches(&v1));
    assert!(range.matches(&v2));
    assert!(!range.matches(&v3));
}

#[test]
fn test_version_range_min_max() {
    let exact = VersionRange::parse("=1.2.3").unwrap();
    assert_eq!(
        exact.min_version().map(|v| v.to_string()),
        Some("1.2.3".to_string())
    );
    assert_eq!(
        exact.max_version().map(|v| v.to_string()),
        Some("1.2.3".to_string())
    );

    let range = VersionRange::parse(">=1.0.0,<2.0.0").unwrap();
    assert_eq!(
        range.min_version().map(|v| v.to_string()),
        Some("1.0.0".to_string())
    );
    assert_eq!(
        range.max_version().map(|v| v.to_string()),
        Some("2.0.0".to_string())
    );

    let any = VersionRange::parse("*").unwrap();
    assert_eq!(any.min_version(), None);
    assert_eq!(any.max_version(), None);
}

#[test]
fn test_multiple_vulnerable_ranges() {
    // Test matching against multiple vulnerable ranges (like a CVE with multiple affected versions)
    let ranges = vec![
        VersionRange::parse(">=1.0.0,<2.0.0").unwrap(),
        VersionRange::parse(">=3.0.0,<3.1.0").unwrap(),
        VersionRange::parse(">=4.0.0,<4.0.5").unwrap(),
    ];

    let test_cases = vec![
        ("1.5.0", true),
        ("2.0.0", false),
        ("3.0.5", true),
        ("3.1.0", false),
        ("4.0.3", true),
        ("4.0.5", false),
        ("5.0.0", false),
    ];

    for (version_str, should_match) in test_cases {
        let version = Version::parse(version_str).unwrap();
        let matches = ranges.iter().any(|r| r.matches(&version));
        assert_eq!(
            matches,
            should_match,
            "Version {} should {} match",
            version_str,
            if should_match { "" } else { "not " }
        );
    }
}

#[test]
fn test_version_tuple_representation() {
    let v = Version::parse("1.2.3").unwrap();
    assert_eq!(v.as_tuple(), (1, 2, 3));
}

#[test]
fn test_osv_maven_version_format() {
    // Test Maven version format parsing
    let osv = OsvVersionRange::parse("[1.0.0,2.0.0]", "maven").unwrap();
    let v1 = Version::parse("1.0.0").unwrap();
    let v2 = Version::parse("1.5.0").unwrap();
    let v3 = Version::parse("2.0.0").unwrap();

    assert!(osv.matches(&v1));
    assert!(osv.matches(&v2));
    assert!(osv.matches(&v3));
}
