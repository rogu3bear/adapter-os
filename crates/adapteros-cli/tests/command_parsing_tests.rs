//! Tests for CLI command parsing and argument validation
//!
//! Tests that CLI commands are properly parsed by clap, validate arguments,
//! and handle edge cases correctly.

use clap::Parser;

// Helper function to parse CLI args from a vector of strings
fn parse_cli(args: Vec<&str>) -> Result<adapteros_cli::app::Cli, clap::Error> {
    adapteros_cli::app::Cli::try_parse_from(args)
}

#[cfg(test)]
mod command_parsing {
    use super::*;

    #[test]
    fn test_global_flags_parsing() {
        // Test JSON flag
        let result = parse_cli(vec!["aosctl", "--json", "adapter-list"]);
        assert!(result.is_ok());
        let cli = result.unwrap();
        assert!(cli.is_json());

        // Test quiet flag
        let result = parse_cli(vec!["aosctl", "--quiet", "adapter-list"]);
        assert!(result.is_ok());
        let cli = result.unwrap();
        assert!(cli.is_quiet());

        // Test verbose flag
        let result = parse_cli(vec!["aosctl", "--verbose", "adapter-list"]);
        assert!(result.is_ok());
        let cli = result.unwrap();
        assert!(cli.is_verbose());

        // Test short flags
        let result = parse_cli(vec!["aosctl", "-q", "-v", "adapter-list"]);
        assert!(result.is_ok());
        let cli = result.unwrap();
        assert!(cli.is_quiet());
        assert!(cli.is_verbose());
    }

    #[test]
    fn test_model_path_parsing() {
        let result = parse_cli(vec![
            "aosctl",
            "--model-path",
            "/path/to/model",
            "adapter-list",
        ]);
        assert!(result.is_ok());
        let cli = result.unwrap();
        assert_eq!(cli.model_path.as_deref(), Some("/path/to/model"));
    }

    #[test]
    fn test_model_backend_parsing() {
        // Default backend
        let result = parse_cli(vec!["aosctl", "adapter-list"]);
        assert!(result.is_ok());
        let cli = result.unwrap();
        assert_eq!(cli.model_backend, "auto");

        // Explicit backend
        let result = parse_cli(vec!["aosctl", "--model-backend", "metal", "adapter-list"]);
        assert!(result.is_ok());
        let cli = result.unwrap();
        assert_eq!(cli.model_backend, "metal");

        // CoreML backend
        let result = parse_cli(vec!["aosctl", "--model-backend", "coreml", "adapter-list"]);
        assert!(result.is_ok());
        let cli = result.unwrap();
        assert_eq!(cli.model_backend, "coreml");

        // MLX backend
        let result = parse_cli(vec!["aosctl", "--model-backend", "mlx", "adapter-list"]);
        assert!(result.is_ok());
        let cli = result.unwrap();
        assert_eq!(cli.model_backend, "mlx");
    }

    #[test]
    fn test_adapter_list_command() {
        // Basic command
        let result = parse_cli(vec!["aosctl", "adapter-list"]);
        assert!(result.is_ok());

        // With tier filter
        let result = parse_cli(vec!["aosctl", "adapter-list", "--tier", "persistent"]);
        assert!(result.is_ok());

        // With include-meta flag
        let result = parse_cli(vec!["aosctl", "adapter-list", "--include-meta"]);
        assert!(result.is_ok());

        // Legacy alias still works
        let result = parse_cli(vec!["aosctl", "list-adapters"]);
        assert!(result.is_ok());
    }

    #[test]
    fn test_adapter_register_command() {
        let result = parse_cli(vec![
            "aosctl",
            "adapter-register",
            "--adapter-id",
            "test_adapter",
            "--aos",
            "/path/to/adapter.aos",
            "--tenant-id",
            "tenant1",
            "--base-model-id",
            "qwen2.5-7b",
            "--tier",
            "persistent",
            "--rank",
            "16",
        ]);
        assert!(result.is_ok());
    }

    #[test]
    fn test_adapter_pin_command() {
        // Without TTL (permanent pin)
        let result = parse_cli(vec![
            "aosctl",
            "adapter-pin",
            "--tenant",
            "dev",
            "--adapter",
            "specialist",
            "--reason",
            "Production critical",
        ]);
        assert!(result.is_ok());

        // With TTL
        let result = parse_cli(vec![
            "aosctl",
            "adapter-pin",
            "--tenant",
            "dev",
            "--adapter",
            "temp_fix",
            "--ttl-hours",
            "24",
            "--reason",
            "Testing",
        ]);
        assert!(result.is_ok());
    }

    #[test]
    fn test_adapter_unpin_command() {
        let result = parse_cli(vec![
            "aosctl",
            "adapter-unpin",
            "--tenant",
            "dev",
            "--adapter",
            "temp_fix",
        ]);
        assert!(result.is_ok());
    }

    #[test]
    fn test_adapter_swap_command() {
        // Dry-run with add and remove
        let result = parse_cli(vec![
            "aosctl",
            "adapter-swap",
            "--tenant",
            "dev",
            "--add",
            "adapter1,adapter2",
            "--remove",
            "old_adapter",
        ]);
        assert!(result.is_ok());

        // Commit swap
        let result = parse_cli(vec![
            "aosctl",
            "adapter-swap",
            "--tenant",
            "dev",
            "--add",
            "specialist",
            "--remove",
            "temp_fix",
            "--commit",
        ]);
        assert!(result.is_ok());

        // Custom timeout
        let result = parse_cli(vec![
            "aosctl",
            "adapter-swap",
            "--tenant",
            "dev",
            "--add",
            "adapter1",
            "--timeout",
            "10000",
        ]);
        assert!(result.is_ok());
    }

    #[test]
    fn test_adapter_info_command() {
        let result = parse_cli(vec!["aosctl", "adapter-info", "specialist"]);
        assert!(result.is_ok());
    }

    #[test]
    fn test_tenant_init_command() {
        let result = parse_cli(vec![
            "aosctl",
            "tenant-init",
            "--id",
            "tenant_dev",
            "--uid",
            "1000",
            "--gid",
            "1000",
        ]);
        assert!(result.is_ok());
    }

    #[test]
    fn test_node_list_command() {
        // Online mode (default) - using subcommand syntax
        let result = parse_cli(vec!["aosctl", "node", "list"]);
        assert!(result.is_ok());
    }

    #[test]
    fn test_missing_required_arguments() {
        // Missing adapter ID for register
        let result = parse_cli(vec![
            "aosctl",
            "adapter-register",
            "--aos",
            "/path/to/adapter.aos",
        ]);
        assert!(result.is_err());

        // Missing tenant for pin
        let result = parse_cli(vec!["aosctl", "adapter-pin", "--adapter", "specialist"]);
        assert!(result.is_err());

        // Missing reason for pin
        let result = parse_cli(vec![
            "aosctl",
            "adapter-pin",
            "--tenant",
            "dev",
            "--adapter",
            "specialist",
        ]);
        assert!(result.is_err());
    }

    #[test]
    fn test_invalid_flag_combinations() {
        // JSON and quiet together (should parse but behavior is undefined)
        let result = parse_cli(vec!["aosctl", "--json", "--quiet", "adapter-list"]);
        assert!(result.is_ok()); // Clap allows this, logic handles precedence
    }

    #[test]
    fn test_help_flag() {
        // Help for main command
        let result = parse_cli(vec!["aosctl", "--help"]);
        assert!(result.is_err()); // Help causes exit
        let err = result.err().expect("expected help error");
        assert_eq!(err.kind(), clap::error::ErrorKind::DisplayHelp);

        // Help for subcommand
        let result = parse_cli(vec!["aosctl", "adapter-list", "--help"]);
        assert!(result.is_err());
        let err = result.err().expect("expected help error");
        assert_eq!(err.kind(), clap::error::ErrorKind::DisplayHelp);
    }

    #[test]
    fn test_version_flag() {
        let result = parse_cli(vec!["aosctl", "--version"]);
        assert!(result.is_err());
        let err = result.err().expect("expected version error");
        assert_eq!(err.kind(), clap::error::ErrorKind::DisplayVersion);
    }

    #[test]
    fn test_unknown_command() {
        let result = parse_cli(vec!["aosctl", "unknown-command"]);
        assert!(result.is_err());
        let err = result.err().expect("expected invalid subcommand error");
        assert_eq!(err.kind(), clap::error::ErrorKind::InvalidSubcommand);
    }

    #[test]
    fn test_unknown_flag() {
        let result = parse_cli(vec!["aosctl", "--unknown-flag", "adapter-list"]);
        assert!(result.is_err());
        let err = result.err().expect("expected unknown argument error");
        assert_eq!(err.kind(), clap::error::ErrorKind::UnknownArgument);
    }

    #[test]
    fn test_comma_separated_values() {
        // Adapter swap with comma-separated adapters
        let result = parse_cli(vec![
            "aosctl",
            "adapter-swap",
            "--tenant",
            "dev",
            "--add",
            "a1,a2,a3",
            "--remove",
            "b1,b2",
        ]);
        assert!(result.is_ok());
    }

    #[test]
    fn test_numeric_argument_validation() {
        // Valid rank
        let result = parse_cli(vec![
            "aosctl",
            "adapter-register",
            "--adapter-id",
            "test",
            "--aos",
            "/path",
            "--tenant-id",
            "t1",
            "--base-model-id",
            "model",
            "--rank",
            "16",
        ]);
        assert!(result.is_ok());

        // Invalid rank (negative)
        let result = parse_cli(vec![
            "aosctl",
            "adapter-register",
            "--adapter-id",
            "test",
            "--aos",
            "/path",
            "--tenant-id",
            "t1",
            "--base-model-id",
            "model",
            "--rank",
            "-1",
        ]);
        assert!(result.is_err());

        // Invalid rank (not a number)
        let result = parse_cli(vec![
            "aosctl",
            "adapter-register",
            "--adapter-id",
            "test",
            "--aos",
            "/path",
            "--tenant-id",
            "t1",
            "--base-model-id",
            "model",
            "--rank",
            "not-a-number",
        ]);
        assert!(result.is_err());
    }

    #[test]
    fn test_default_values() {
        let result = parse_cli(vec![
            "aosctl",
            "adapter-register",
            "--adapter-id",
            "test",
            "--aos",
            "/path",
            "--tenant-id",
            "t1",
            "--base-model-id",
            "model",
            "--rank",
            "16",
        ]);
        assert!(result.is_ok());
        // Note: We can't directly test the tier default without access to the parsed command
        // This would require matching on the Commands enum
    }
}
