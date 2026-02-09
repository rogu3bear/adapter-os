use crate::api_error::ApiError;
use std::net::{IpAddr, Ipv4Addr, Ipv6Addr, SocketAddr};
use std::sync::Arc;
use std::time::Duration;

pub const DEFAULT_CONNECT_TIMEOUT: Duration = Duration::from_millis(500);
pub const DEFAULT_TOTAL_TIMEOUT: Duration = Duration::from_secs(5);
pub const MAX_ERROR_BODY_CHARS: usize = 300;

pub fn truncate_body_chars(s: &str, max_chars: usize) -> String {
    let mut out: String = s.chars().take(max_chars).collect();
    if s.chars().count() > max_chars {
        out.push_str("...");
    }
    out
}

/// Build an HTTP client with optional SSRF protection.
///
/// When `ssrf_guard` is true, the client installs a custom DNS resolver that
/// rejects connections to private/reserved IP ranges before a TCP connection
/// is established. This prevents Server-Side Request Forgery (SSRF) attacks
/// where an attacker-controlled URL resolves to an internal service.
///
/// Blocked ranges:
/// - 127.0.0.0/8, 10.0.0.0/8, 172.16.0.0/12, 192.168.0.0/16 (RFC 1918)
/// - 169.254.0.0/16 (link-local / cloud metadata)
/// - 0.0.0.0/8 (current network)
/// - fc00::/7 (IPv6 ULA), ::1/128 (IPv6 loopback)
///
/// Set `ssrf_guard` to false (via `server.ssrf_protection = false` in config)
/// for air-gapped deployments where webhook targets are on a private network.
pub fn build_reqwest_client(
    connect_timeout: Duration,
    total_timeout: Duration,
    ssrf_guard: bool,
) -> Result<reqwest::Client, ApiError> {
    let mut builder = reqwest::Client::builder()
        .connect_timeout(connect_timeout)
        .timeout(total_timeout);

    if ssrf_guard {
        builder = builder.dns_resolver(Arc::new(SsrfGuardResolver::new()));
    }

    builder.build().map_err(|e| {
        ApiError::internal("failed to build HTTP client").with_redacted_details(e.to_string())
    })
}

// ---------------------------------------------------------------------------
// SSRF guard: custom DNS resolver that blocks private/reserved IP ranges
// ---------------------------------------------------------------------------

/// Returns true if the IP address belongs to a private or reserved range
/// that should not be reachable from outbound HTTP requests.
fn is_private_or_reserved(ip: &IpAddr) -> bool {
    match ip {
        IpAddr::V4(v4) => is_private_v4(v4),
        IpAddr::V6(v6) => is_private_v6(v6),
    }
}

fn is_private_v4(ip: &Ipv4Addr) -> bool {
    let octets = ip.octets();
    // 127.0.0.0/8 - loopback
    if octets[0] == 127 {
        return true;
    }
    // 10.0.0.0/8 - RFC 1918
    if octets[0] == 10 {
        return true;
    }
    // 172.16.0.0/12 - RFC 1918
    if octets[0] == 172 && (16..=31).contains(&octets[1]) {
        return true;
    }
    // 192.168.0.0/16 - RFC 1918
    if octets[0] == 192 && octets[1] == 168 {
        return true;
    }
    // 169.254.0.0/16 - link-local (includes cloud metadata 169.254.169.254)
    if octets[0] == 169 && octets[1] == 254 {
        return true;
    }
    // 0.0.0.0/8 - current network
    if octets[0] == 0 {
        return true;
    }
    false
}

fn is_private_v6(ip: &Ipv6Addr) -> bool {
    // ::1 - IPv6 loopback
    if ip.is_loopback() {
        return true;
    }
    // fc00::/7 - Unique Local Addresses
    let segments = ip.segments();
    if segments[0] & 0xfe00 == 0xfc00 {
        return true;
    }
    // IPv4-mapped IPv6 addresses (::ffff:x.x.x.x) - check the embedded v4
    if let Some(v4) = ip.to_ipv4_mapped() {
        return is_private_v4(&v4);
    }
    false
}

/// A [`reqwest::dns::Resolve`] implementation that performs standard system DNS
/// resolution and then rejects any results in private/reserved IP ranges.
#[derive(Debug, Clone)]
struct SsrfGuardResolver;

impl SsrfGuardResolver {
    fn new() -> Self {
        Self
    }
}

impl reqwest::dns::Resolve for SsrfGuardResolver {
    fn resolve(&self, name: reqwest::dns::Name) -> reqwest::dns::Resolving {
        // Perform standard DNS resolution using tokio's built-in resolver,
        // then filter the results to reject private/reserved IPs.
        let name_str = name.as_str().to_owned();
        Box::pin(async move {
            // Use tokio's DNS resolution (same as reqwest's default)
            let addrs: Vec<SocketAddr> = tokio::net::lookup_host(format!("{}:0", name_str))
                .await
                .map_err(|e| -> Box<dyn std::error::Error + Send + Sync> { Box::new(e) })?
                .collect();

            // Check all resolved addresses - if ANY resolves to a private range, reject.
            // Checking all (not just filtering) prevents DNS rebinding where one A record
            // is public and another is private.
            let blocked: Vec<IpAddr> = addrs
                .iter()
                .map(|sa| sa.ip())
                .filter(|ip| is_private_or_reserved(ip))
                .collect();

            if !blocked.is_empty() {
                let blocked_strs: Vec<String> = blocked.iter().map(|ip| ip.to_string()).collect();
                return Err(Box::new(SsrfBlockedError {
                    host: name_str,
                    blocked_ips: blocked_strs,
                })
                    as Box<dyn std::error::Error + Send + Sync>);
            }

            if addrs.is_empty() {
                return Err(Box::new(std::io::Error::new(
                    std::io::ErrorKind::AddrNotAvailable,
                    format!("DNS resolution for '{}' returned no addresses", name_str),
                ))
                    as Box<dyn std::error::Error + Send + Sync>);
            }

            let addrs: reqwest::dns::Addrs = Box::new(addrs.into_iter());
            Ok(addrs)
        })
    }
}

/// Error returned when DNS resolution yields a private/reserved IP.
#[derive(Debug)]
struct SsrfBlockedError {
    host: String,
    blocked_ips: Vec<String>,
}

impl std::fmt::Display for SsrfBlockedError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "SSRF protection: '{}' resolved to private/reserved IP address(es) [{}]; \
             set server.ssrf_protection = false in config to allow private-network targets",
            self.host,
            self.blocked_ips.join(", ")
        )
    }
}

impl std::error::Error for SsrfBlockedError {}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_loopback_v4_blocked() {
        assert!(is_private_or_reserved(&IpAddr::V4(Ipv4Addr::new(
            127, 0, 0, 1
        ))));
        assert!(is_private_or_reserved(&IpAddr::V4(Ipv4Addr::new(
            127, 255, 255, 255
        ))));
    }

    #[test]
    fn test_rfc1918_10_blocked() {
        assert!(is_private_or_reserved(&IpAddr::V4(Ipv4Addr::new(
            10, 0, 0, 1
        ))));
        assert!(is_private_or_reserved(&IpAddr::V4(Ipv4Addr::new(
            10, 255, 255, 255
        ))));
    }

    #[test]
    fn test_rfc1918_172_blocked() {
        assert!(is_private_or_reserved(&IpAddr::V4(Ipv4Addr::new(
            172, 16, 0, 1
        ))));
        assert!(is_private_or_reserved(&IpAddr::V4(Ipv4Addr::new(
            172, 31, 255, 255
        ))));
        // 172.15.x.x is NOT private
        assert!(!is_private_or_reserved(&IpAddr::V4(Ipv4Addr::new(
            172, 15, 0, 1
        ))));
        // 172.32.x.x is NOT private
        assert!(!is_private_or_reserved(&IpAddr::V4(Ipv4Addr::new(
            172, 32, 0, 1
        ))));
    }

    #[test]
    fn test_rfc1918_192_168_blocked() {
        assert!(is_private_or_reserved(&IpAddr::V4(Ipv4Addr::new(
            192, 168, 0, 1
        ))));
        assert!(is_private_or_reserved(&IpAddr::V4(Ipv4Addr::new(
            192, 168, 255, 255
        ))));
    }

    #[test]
    fn test_link_local_blocked() {
        // Cloud metadata endpoint
        assert!(is_private_or_reserved(&IpAddr::V4(Ipv4Addr::new(
            169, 254, 169, 254
        ))));
        assert!(is_private_or_reserved(&IpAddr::V4(Ipv4Addr::new(
            169, 254, 0, 1
        ))));
    }

    #[test]
    fn test_current_network_blocked() {
        assert!(is_private_or_reserved(&IpAddr::V4(Ipv4Addr::new(
            0, 0, 0, 0
        ))));
        assert!(is_private_or_reserved(&IpAddr::V4(Ipv4Addr::new(
            0, 0, 0, 1
        ))));
    }

    #[test]
    fn test_public_ip_allowed() {
        assert!(!is_private_or_reserved(&IpAddr::V4(Ipv4Addr::new(
            8, 8, 8, 8
        ))));
        assert!(!is_private_or_reserved(&IpAddr::V4(Ipv4Addr::new(
            1, 1, 1, 1
        ))));
        assert!(!is_private_or_reserved(&IpAddr::V4(Ipv4Addr::new(
            93, 184, 216, 34
        ))));
    }

    #[test]
    fn test_ipv6_loopback_blocked() {
        assert!(is_private_or_reserved(&IpAddr::V6(Ipv6Addr::LOCALHOST)));
    }

    #[test]
    fn test_ipv6_ula_blocked() {
        // fc00::1
        assert!(is_private_or_reserved(&IpAddr::V6(Ipv6Addr::new(
            0xfc00, 0, 0, 0, 0, 0, 0, 1
        ))));
        // fd12::1
        assert!(is_private_or_reserved(&IpAddr::V6(Ipv6Addr::new(
            0xfd12, 0, 0, 0, 0, 0, 0, 1
        ))));
    }

    #[test]
    fn test_ipv6_public_allowed() {
        // 2001:4860:4860::8888 (Google DNS)
        assert!(!is_private_or_reserved(&IpAddr::V6(Ipv6Addr::new(
            0x2001, 0x4860, 0x4860, 0, 0, 0, 0, 0x8888
        ))));
    }

    #[test]
    fn test_ipv4_mapped_v6_private_blocked() {
        // ::ffff:127.0.0.1
        let mapped = Ipv6Addr::new(0, 0, 0, 0, 0, 0xffff, 0x7f00, 0x0001);
        assert!(is_private_or_reserved(&IpAddr::V6(mapped)));
        // ::ffff:10.0.0.1
        let mapped = Ipv6Addr::new(0, 0, 0, 0, 0, 0xffff, 0x0a00, 0x0001);
        assert!(is_private_or_reserved(&IpAddr::V6(mapped)));
    }

    #[test]
    fn test_ipv4_mapped_v6_public_allowed() {
        // ::ffff:8.8.8.8
        let mapped = Ipv6Addr::new(0, 0, 0, 0, 0, 0xffff, 0x0808, 0x0808);
        assert!(!is_private_or_reserved(&IpAddr::V6(mapped)));
    }

    #[test]
    fn test_ssrf_error_display() {
        let err = SsrfBlockedError {
            host: "evil.example.com".to_string(),
            blocked_ips: vec!["127.0.0.1".to_string(), "10.0.0.1".to_string()],
        };
        let msg = format!("{}", err);
        assert!(msg.contains("evil.example.com"));
        assert!(msg.contains("127.0.0.1"));
        assert!(msg.contains("ssrf_protection = false"));
    }

    #[test]
    fn test_build_client_without_ssrf_guard() {
        let client =
            build_reqwest_client(Duration::from_millis(100), Duration::from_secs(1), false);
        assert!(client.is_ok());
    }

    #[test]
    fn test_build_client_with_ssrf_guard() {
        let client = build_reqwest_client(Duration::from_millis(100), Duration::from_secs(1), true);
        assert!(client.is_ok());
    }
}
