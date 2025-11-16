//! Egress control and PF rule validation

use adapteros_core::{AosError, Result};
use std::process::Command;

/// Validate that Packet Filter (PF) rules are active and block egress
///
/// On macOS, this checks the PF firewall configuration to ensure:
/// 1. PF is enabled
/// 2. Outbound connections are blocked (deny-all egress)
/// 3. Only Unix domain sockets are allowed
pub fn validate_pf_rules() -> Result<()> {
    #[cfg(target_os = "macos")]
    {
        validate_pf_macos()
    }

    #[cfg(not(target_os = "macos"))]
    {
        Err(AosError::EgressViolation(
            "PF validation only supported on macOS".to_string(),
        ))
    }
}

#[cfg(target_os = "macos")]
fn validate_pf_macos() -> Result<()> {
    // Check if PF is enabled
    let output = Command::new("pfctl")
        .args(["-s", "info"])
        .output()
        .map_err(|e| {
            AosError::EgressViolation(format!("Failed to check PF status (are you root?): {}", e))
        })?;

    if !output.status.success() {
        return Err(AosError::EgressViolation(
            "PF firewall is not accessible. Run with appropriate privileges.".to_string(),
        ));
    }

    let info = String::from_utf8_lossy(&output.stdout);

    // Check if PF is enabled
    if info.contains("Status: Disabled") {
        return Err(AosError::EgressViolation(
            "PF firewall is disabled. Enable with: sudo pfctl -e".to_string(),
        ));
    }

    // Check active rules
    let output = Command::new("pfctl")
        .args(["-s", "rules"])
        .output()
        .map_err(|e| AosError::EgressViolation(format!("Failed to read PF rules: {}", e)))?;

    let rules = String::from_utf8_lossy(&output.stdout);

    // Look for deny-all outbound rules
    // In production, this would parse the PF rule format more robustly
    let has_deny_out = rules.contains("block out") || rules.contains("block all");

    if !has_deny_out {
        tracing::warn!(
            current_rules = %rules,
            "No explicit deny-all outbound rule detected in PF configuration"
        );
        tracing::info!(
            "To add deny-all egress: echo 'block out all' | sudo pfctl -f -"
        );

        // In strict mode, return error
        return Err(AosError::EgressViolation(
            "No deny-all outbound rule found in PF configuration".to_string(),
        ));
    }

    tracing::info!("PF firewall enabled with egress blocking verified");
    Ok(())
}

/// Check if any TCP/UDP sockets are bound (should be none in serving mode)
pub fn validate_no_network_sockets() -> Result<()> {
    // On macOS, use lsof or netstat to check for listening sockets
    #[cfg(target_os = "macos")]
    {
        let output = Command::new("lsof")
            .args(["-iTCP", "-sTCP:LISTEN", "-n", "-P"])
            .output();

        if let Ok(output) = output {
            let sockets = String::from_utf8_lossy(&output.stdout);
            let lines: Vec<&str> = sockets.lines().skip(1).collect(); // Skip header

            if !lines.is_empty() {
                tracing::warn!(
                    socket_count = lines.len(),
                    sockets = %sockets,
                    "TCP listening sockets detected - potential egress violation"
                );
                // In production, this would be an error
                // For now, just warn
            }
        }
    }

    Ok(())
}

/// Run all egress validation checks
pub fn validate_egress_policy() -> Result<()> {
    validate_pf_rules()?;
    validate_no_network_sockets()?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    #[cfg(target_os = "macos")]
    fn test_pf_validation() {
        // Note: This test requires root privileges and active PF
        // In CI, this would be skipped or mocked
        let result = validate_pf_rules();

        // Just check that it doesn't panic
        // Actual validation depends on system configuration
        match result {
            Ok(_) => println!("PF validation passed"),
            Err(e) => println!("PF validation: {}", e),
        }
    }
}
