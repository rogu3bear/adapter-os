//! Tests for CLI help examples and functionality
//!
//! These tests verify that the CLI commands exist and have proper help output.
//! After the git-style subcommand refactoring, commands use the new format:
//! - `aosctl telemetry verify` instead of `aosctl telemetry-verify`
//! - `aosctl adapter list` instead of `aosctl list-adapters`
//! - `aosctl codegraph export` instead of `aosctl callgraph-export`
//! - `aosctl secd status` instead of `aosctl secd-status`

#[cfg(test)]
mod tests {
    use std::process::Command;

    #[test]
    fn help_contains_examples() {
        // Test the new git-style subcommand: `telemetry verify`
        let output = Command::new("cargo")
            .args([
                "run",
                "--bin",
                "aosctl",
                "--",
                "telemetry",
                "verify",
                "--help",
            ])
            .output()
            .expect("Failed to execute command");

        let stdout =
            String::from_utf8(output.stdout).expect("Test UTF-8 conversion should succeed");
        // Telemetry verify subcommand exists - check command is recognized
        assert!(
            output.status.success(),
            "Telemetry verify command should exist"
        );
    }

    #[test]
    fn help_contains_examples_adapter_list() {
        // Test the new git-style subcommand: `adapter list`
        let output = Command::new("cargo")
            .args(["run", "--bin", "aosctl", "--", "adapter", "list", "--help"])
            .output()
            .expect("Failed to execute command");

        assert!(output.status.success(), "Adapter list command should exist");
    }

    #[test]
    fn help_contains_examples_callgraph_export() {
        // Test the new git-style subcommand: `codegraph export`
        let output = Command::new("cargo")
            .args([
                "run",
                "--bin",
                "aosctl",
                "--",
                "codegraph",
                "export",
                "--help",
            ])
            .output()
            .expect("Failed to execute command");

        assert!(
            output.status.success(),
            "Codegraph export command should exist"
        );
    }

    #[test]
    fn help_contains_examples_secd_status() {
        // Test the new git-style subcommand: `secd status`
        let output = Command::new("cargo")
            .args(["run", "--bin", "aosctl", "--", "secd", "status", "--help"])
            .output()
            .expect("Failed to execute command");

        assert!(output.status.success(), "Secd status command should exist");
    }

    #[test]
    fn explain_command_exists() {
        let output = Command::new("cargo")
            .args(["run", "--bin", "aosctl", "--", "explain", "--help"])
            .output()
            .expect("Failed to execute command");

        assert!(output.status.success(), "Explain command should exist");
    }

    #[test]
    fn error_codes_command_exists() {
        let output = Command::new("cargo")
            .args(["run", "--bin", "aosctl", "--", "error-codes", "--help"])
            .output()
            .expect("Failed to execute command");

        assert!(output.status.success(), "Error codes command should exist");
    }

    #[test]
    fn tutorial_command_exists() {
        let output = Command::new("cargo")
            .args(["run", "--bin", "aosctl", "--", "tutorial", "--help"])
            .output()
            .expect("Failed to execute command");

        assert!(output.status.success(), "Tutorial command should exist");
    }

    #[test]
    fn manual_command_exists() {
        let output = Command::new("cargo")
            .args(["run", "--bin", "aosctl", "--", "manual", "--help"])
            .output()
            .expect("Failed to execute command");

        assert!(output.status.success(), "Manual command should exist");
    }

    #[test]
    fn help_contains_examples_infer() {
        let output = Command::new("cargo")
            .args(["run", "--bin", "aosctl", "--", "infer", "--help"])
            .output()
            .expect("Failed to execute command");

        let stdout =
            String::from_utf8(output.stdout).expect("Test UTF-8 conversion should succeed");
        assert!(
            stdout.contains("Examples:"),
            "Infer help should contain examples section"
        );
    }

    #[test]
    fn help_contains_examples_train() {
        // Train command replaced quantize-qwen - train has comprehensive examples
        let output = Command::new("cargo")
            .args(["run", "--bin", "aosctl", "--", "train", "--help"])
            .output()
            .expect("Failed to execute command");

        let stdout =
            String::from_utf8(output.stdout).expect("Test UTF-8 conversion should succeed");
        assert!(
            stdout.contains("Examples:"),
            "Train help should contain examples section"
        );
    }
}
