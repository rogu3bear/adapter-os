use crate::config::SecurityConfig;
use anyhow::{bail, Result};
use tracing::{info, warn};

/// Packet Filter Guard: ensures egress is blocked before serving
pub struct PfGuard;

impl PfGuard {
    /// Run preflight check to ensure PF deny rules are active
    pub fn preflight(config: &SecurityConfig) -> Result<()> {
        if !config.require_pf_deny {
            warn!("PF egress check disabled in configuration");
            return Ok(());
        }

        #[cfg(target_os = "macos")]
        {
            Self::check_pf_macos()?;
        }

        #[cfg(target_os = "linux")]
        {
            Self::check_iptables_linux()?;
        }

        #[cfg(not(any(target_os = "macos", target_os = "linux")))]
        {
            bail!("PF egress check not implemented for this platform");
        }

        info!("Security preflight passed: egress blocked");
        Ok(())
    }

    #[cfg(target_os = "macos")]
    fn check_pf_macos() -> Result<()> {
        use std::process::Command;

        // Check if PF is enabled
        let output = Command::new("pfctl").arg("-s").arg("info").output()?;

        if !output.status.success() {
            bail!("Failed to query PF status: not running as root?");
        }

        let stdout = String::from_utf8_lossy(&output.stdout);

        if !stdout.contains("Status: Enabled") {
            bail!("PF is not enabled - refusing to start control plane");
        }

        // Verify specific deny rules for outbound traffic
        let rules_output = Command::new("pfctl")
            .arg("-s")
            .arg("rules")
            .arg("-v")
            .output()?;

        if !rules_output.status.success() {
            bail!("Failed to query PF rules: not running as root?");
        }

        let rules_stdout = String::from_utf8_lossy(&rules_output.stdout);
        let mut tcp_blocked = false;
        let mut udp_blocked = false;
        let mut rule_count = 0;

        for line in rules_stdout.lines() {
            // Look for outbound block rules
            if line.contains("block out") {
                rule_count += 1;
                if line.contains("proto tcp") || line.contains("all") {
                    tcp_blocked = true;
                }
                if line.contains("proto udp") || line.contains("all") {
                    udp_blocked = true;
                }
            }
            // Check for bypass rules that would allow egress
            if line.contains("pass out") && !line.contains("lo0") {
                warn!("Found pass-out rule that may bypass blocks: {}", line);
            }
        }

        if rule_count == 0 {
            bail!("No outbound block rules found in PF configuration");
        }

        if !tcp_blocked {
            bail!("TCP egress not blocked - missing 'block out proto tcp' rule");
        }

        if !udp_blocked {
            bail!("UDP egress not blocked - missing 'block out proto udp' rule");
        }

        info!(
            "PF verification: {} block rules active (TCP/UDP blocked)",
            rule_count
        );
        Ok(())
    }

    #[cfg(target_os = "linux")]
    fn check_iptables_linux() -> Result<()> {
        use std::process::Command;

        // Check iptables OUTPUT chain for DROP policy or rules
        let output = Command::new("iptables")
            .arg("-L")
            .arg("OUTPUT")
            .arg("-n")
            .output()?;

        if !output.status.success() {
            bail!("Failed to query iptables: not running as root?");
        }

        let stdout = String::from_utf8_lossy(&output.stdout);

        // Check for DROP/REJECT policy
        let has_drop_policy = stdout.lines().any(|line| {
            line.starts_with("Chain OUTPUT")
                && (line.contains("policy DROP") || line.contains("policy REJECT"))
        });

        // Count specific DROP/REJECT rules
        let mut tcp_rules = 0;
        let mut udp_rules = 0;

        for line in stdout.lines() {
            if line.contains("DROP") || line.contains("REJECT") {
                if line.contains("tcp") {
                    tcp_rules += 1;
                }
                if line.contains("udp") {
                    udp_rules += 1;
                }
            }
        }

        if !has_drop_policy && tcp_rules == 0 && udp_rules == 0 {
            bail!("No egress blocking found: OUTPUT chain needs DROP/REJECT policy or rules");
        }

        if tcp_rules == 0 && !has_drop_policy {
            bail!("TCP egress not blocked - missing DROP/REJECT rules for tcp protocol");
        }

        if udp_rules == 0 && !has_drop_policy {
            bail!("UDP egress not blocked - missing DROP/REJECT rules for udp protocol");
        }

        info!(
            "iptables verification: OUTPUT chain configured (policy: {}, TCP rules: {}, UDP rules: {})",
            if has_drop_policy { "DROP/REJECT" } else { "default" },
            tcp_rules,
            udp_rules
        );

        Ok(())
    }
}
