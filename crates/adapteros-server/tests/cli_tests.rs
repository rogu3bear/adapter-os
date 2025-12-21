//! CLI module tests

use adapteros_server::cli::{normalize_jwt_mode, Cli};
use clap::Parser;

#[test]
fn normalize_jwt_mode_hmac_lowercase() {
    assert_eq!(normalize_jwt_mode("hmac"), "hmac");
}

#[test]
fn normalize_jwt_mode_hs256_uppercase() {
    assert_eq!(normalize_jwt_mode("HS256"), "hmac");
}

#[test]
fn normalize_jwt_mode_eddsa_lowercase() {
    assert_eq!(normalize_jwt_mode("eddsa"), "eddsa");
}

#[test]
fn normalize_jwt_mode_ed25519_lowercase() {
    assert_eq!(normalize_jwt_mode("ed25519"), "eddsa");
}

#[test]
fn normalize_jwt_mode_unknown_passthrough() {
    assert_eq!(normalize_jwt_mode("unknown"), "unknown");
}

#[test]
fn cli_parse_minimal_args() {
    let cli = Cli::try_parse_from(["aos-cp"]).expect("Failed to parse CLI with minimal args");
    assert_eq!(cli.config, "configs/cp.toml");
}

#[test]
fn cli_default_values() {
    let cli = Cli::try_parse_from(["aos-cp"]).expect("Failed to parse CLI");

    assert_eq!(cli.config, "configs/cp.toml");
    assert!(!cli.migrate_only);
    assert!(!cli.generate_openapi);
    assert!(cli.single_writer);
    assert!(cli.pid_file.is_none());
    assert!(cli.manifest_path.is_none());
    assert!(!cli.strict);
}
