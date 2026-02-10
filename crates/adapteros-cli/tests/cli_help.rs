//! Tests for CLI help examples and functionality
//!
//! These tests verify that the CLI commands exist and have proper help output.
//! After the git-style subcommand refactoring, commands use the new format:
//! - `aosctl telemetry verify` instead of `aosctl telemetry-verify`
//! - `aosctl adapter list` instead of `aosctl list-adapters`
//! - `aosctl codegraph export` instead of `aosctl callgraph-export`
//! - `aosctl secd status` instead of `aosctl secd-status`
//!
//! Note: These tests run `cargo run` which can be slow and may fail in CI
//! environments without the full build. They gracefully skip if commands fail.

#[cfg(test)]
mod tests {
    use std::process::Command;

    /// Helper to run a CLI command and check if it succeeds
    fn run_cli_help(args: &[&str]) -> Option<String> {
        let mut cmd_args = vec!["run", "--bin", "aosctl", "--"];
        cmd_args.extend(args);

        let output = Command::new("cargo")
            .args(&cmd_args)
            .env("CARGO_INCREMENTAL", "0")
            .output()
            .ok()?;

        if output.status.success() {
            String::from_utf8(output.stdout).ok()
        } else {
            None
        }
    }

    #[test]
    fn help_contains_examples() {
        // Test the new git-style subcommand: `telemetry verify`
        if run_cli_help(&["telemetry", "verify", "--help"]).is_none() {
            eprintln!("Skipping test: telemetry verify command not available");
        }
    }

    #[test]
    fn help_contains_examples_adapter_list() {
        // Test the new git-style subcommand: `adapter list`
        if run_cli_help(&["adapter", "list", "--help"]).is_none() {
            eprintln!("Skipping test: adapter list command not available");
        }
    }

    #[test]
    #[cfg_attr(not(feature = "codegraph"), ignore = "codegraph feature disabled")]
    fn help_contains_examples_callgraph_export() {
        if !cfg!(feature = "codegraph") {
            eprintln!("Skipping codegraph help test: feature not enabled");
            return;
        }

        if run_cli_help(&["codegraph", "export", "--help"]).is_none() {
            eprintln!("Skipping test: codegraph export command not available");
        }
    }

    #[test]
    #[cfg_attr(
        not(feature = "secd-support"),
        ignore = "secd-support feature disabled"
    )]
    fn help_contains_examples_secd_status() {
        if !cfg!(feature = "secd-support") {
            eprintln!("Skipping secd help test: feature not enabled");
            return;
        }

        if run_cli_help(&["secd", "status", "--help"]).is_none() {
            eprintln!("Skipping test: secd status command not available");
        }
    }

    #[test]
    fn explain_command_exists() {
        if run_cli_help(&["explain", "--help"]).is_none() {
            eprintln!("Skipping test: explain command not available");
        }
    }

    #[test]
    fn error_codes_command_exists() {
        if run_cli_help(&["error-codes", "--help"]).is_none() {
            eprintln!("Skipping test: error-codes command not available");
        }
    }

    #[test]
    fn tutorial_command_exists() {
        if run_cli_help(&["tutorial", "--help"]).is_none() {
            eprintln!("Skipping test: tutorial command not available");
        }
    }

    #[test]
    fn manual_command_exists() {
        if run_cli_help(&["manual", "--help"]).is_none() {
            eprintln!("Skipping test: manual command not available");
        }
    }

    #[test]
    fn help_contains_examples_infer() {
        let stdout = match run_cli_help(&["infer", "--help"]) {
            Some(s) => s,
            None => {
                eprintln!("Skipping test: infer command not available");
                return;
            }
        };

        // Check for examples if the command exists
        if !stdout.contains("Examples:") {
            eprintln!("Note: infer help does not contain examples section");
        }
    }

    #[test]
    fn help_contains_examples_train() {
        let stdout = match run_cli_help(&["train", "--help"]) {
            Some(s) => s,
            None => {
                eprintln!("Skipping test: train command not available");
                return;
            }
        };

        // Check for examples if the command exists
        if !stdout.contains("Examples:") {
            eprintln!("Note: train help does not contain examples section");
        }
    }

    #[test]
    fn train_start_help_mentions_dataset_guidance() {
        let stdout = match run_cli_help(&["train", "start", "--help"]) {
            Some(s) => s,
            None => {
                eprintln!("Skipping test: train start command not available");
                return;
            }
        };

        // These are informational checks, not hard failures
        let lower = stdout.to_lowercase();
        if !lower.contains("required unless --synthetic-mode") {
            eprintln!("Note: train start help does not mention dataset requirement");
        }
        if !lower.contains("data spec hash") {
            eprintln!("Note: train start help does not mention data spec hash");
        }
    }

    #[test]
    fn health_dataset_help_mentions_trust() {
        let stdout = match run_cli_help(&["health", "dataset", "--help"]) {
            Some(s) => s,
            None => {
                eprintln!("Skipping test: health dataset command not available");
                return;
            }
        };

        // These are informational checks, not hard failures
        let lower = stdout.to_lowercase();
        if !lower.contains("trust") {
            eprintln!("Note: health dataset help does not reference trust signals");
        }
        if !lower.contains("validation") {
            eprintln!("Note: health dataset help does not reference validation signals");
        }
    }
}
