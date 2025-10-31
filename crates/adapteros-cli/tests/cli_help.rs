//! Tests for CLI help examples and functionality

#[cfg(test)]
mod tests {
    use std::process::Command;

    #[test]
    fn help_contains_examples() {
        let output = Command::new("cargo")
            .args(["run", "--bin", "aosctl", "--", "telemetry-verify", "--help"])
            .output()
            .expect("Failed to execute command");

        let stdout =
            String::from_utf8(output.stdout).expect("Test UTF-8 conversion should succeed");
        assert!(
            stdout.contains("Examples:"),
            "Help should contain examples section"
        );
    }

    #[test]
    fn help_contains_examples_adapter_list() {
        let output = Command::new("cargo")
            .args(["run", "--bin", "aosctl", "--", "list-adapters", "--help"])
            .output()
            .expect("Failed to execute command");

        let stdout =
            String::from_utf8(output.stdout).expect("Test UTF-8 conversion should succeed");
        assert!(
            stdout.contains("Examples:"),
            "Adapter list help should contain examples"
        );
    }

    #[test]
    fn help_contains_examples_callgraph_export() {
        let output = Command::new("cargo")
            .args(["run", "--bin", "aosctl", "--", "callgraph-export", "--help"])
            .output()
            .expect("Failed to execute command");

        let stdout =
            String::from_utf8(output.stdout).expect("Test UTF-8 conversion should succeed");
        assert!(
            stdout.contains("Examples:"),
            "Callgraph export help should contain examples"
        );
    }

    #[test]
    fn help_contains_examples_secd_status() {
        let output = Command::new("cargo")
            .args(["run", "--bin", "aosctl", "--", "secd-status", "--help"])
            .output()
            .expect("Failed to execute command");

        let stdout =
            String::from_utf8(output.stdout).expect("Test UTF-8 conversion should succeed");
        assert!(
            stdout.contains("Examples:"),
            "Secd status help should contain examples"
        );
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
    fn help_contains_examples_quantize_qwen() {
        let output = Command::new("cargo")
            .args(["run", "--bin", "aosctl", "--", "quantize-qwen", "--help"])
            .output()
            .expect("Failed to execute command");

        let stdout =
            String::from_utf8(output.stdout).expect("Test UTF-8 conversion should succeed");
        assert!(
            stdout.contains("Examples:"),
            "Quantize-qwen help should contain examples section"
        );
    }
}
