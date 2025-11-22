//! IP Address Extraction from HTTP Headers
//!
//! Extracts client IP addresses from HTTP request headers for audit logging.
//! Supports reverse proxy scenarios (X-Forwarded-For, X-Real-IP).
//!
//! # Security Note
//! Only trust X-Forwarded-For headers if your server is behind a trusted reverse proxy.
//! In production, validate that the proxy sets these headers correctly.

use axum::http::HeaderMap;

/// Extract client IP address from request headers
///
/// Checks headers in priority order:
/// 1. `X-Forwarded-For` (takes first IP, which is the client)
/// 2. `X-Real-IP`
/// 3. Returns `None` if no IP headers found (e.g., UDS socket connections)
///
/// # Examples
///
/// ```no_run
/// use axum::http::HeaderMap;
/// use adapteros_server_api::ip_extraction::extract_client_ip;
///
/// let mut headers = HeaderMap::new();
/// headers.insert("x-forwarded-for", "192.168.1.100, 10.0.0.1".parse().expect("Invalid IP address in test"));
///
/// let ip = extract_client_ip(&headers);
/// assert_eq!(ip, Some("192.168.1.100".to_string()));
/// ```
///
/// # Reverse Proxy Headers
///
/// **X-Forwarded-For Format:** `client_ip, proxy1_ip, proxy2_ip, ...`
/// - We extract the first IP (leftmost) which represents the original client
/// - Subsequent IPs are intermediate proxies
///
/// **X-Real-IP Format:** `client_ip`
/// - Single IP address set by the reverse proxy
///
/// # UDS Connections
///
/// Unix Domain Socket connections do not have IP addresses.
/// Returns `None` for UDS connections, which is acceptable for audit logging.
pub fn extract_client_ip(headers: &HeaderMap) -> Option<String> {
    // Check X-Forwarded-For (RFC 7239)
    // Format: "client, proxy1, proxy2"
    if let Some(forwarded) = headers.get("x-forwarded-for") {
        if let Ok(value) = forwarded.to_str() {
            // Take first IP (client)
            if let Some(client_ip) = value.split(',').next() {
                let trimmed = client_ip.trim();
                if !trimmed.is_empty() {
                    return Some(trimmed.to_string());
                }
            }
        }
    }

    // Check X-Real-IP (nginx standard)
    if let Some(real_ip) = headers.get("x-real-ip") {
        if let Ok(value) = real_ip.to_str() {
            let trimmed = value.trim();
            if !trimmed.is_empty() {
                return Some(trimmed.to_string());
            }
        }
    }

    // No IP available (UDS socket or missing headers)
    None
}

/// Client IP address stored in request extensions
///
/// Injected by `auth_middleware` after extracting from headers.
/// Handlers can access this via `Extension<Option<ClientIp>>`.
#[derive(Clone, Debug)]
pub struct ClientIp(pub String);

#[cfg(test)]
mod tests {
    use super::*;
    use axum::http::HeaderMap;

    #[test]
    fn test_extract_from_x_forwarded_for() {
        let mut headers = HeaderMap::new();
        headers.insert(
            "x-forwarded-for",
            "203.0.113.1, 10.0.0.1, 192.168.1.1"
                .parse()
                .expect("Invalid IP address in test"),
        );

        let ip = extract_client_ip(&headers);
        assert_eq!(ip, Some("203.0.113.1".to_string()));
    }

    #[test]
    fn test_extract_from_x_real_ip() {
        let mut headers = HeaderMap::new();
        headers.insert(
            "x-real-ip",
            "203.0.113.42".parse().expect("Invalid IP address in test"),
        );

        let ip = extract_client_ip(&headers);
        assert_eq!(ip, Some("203.0.113.42".to_string()));
    }

    #[test]
    fn test_x_forwarded_for_priority_over_x_real_ip() {
        let mut headers = HeaderMap::new();
        headers.insert(
            "x-forwarded-for",
            "203.0.113.1".parse().expect("Invalid IP address in test"),
        );
        headers.insert(
            "x-real-ip",
            "203.0.113.2".parse().expect("Invalid IP address in test"),
        );

        let ip = extract_client_ip(&headers);
        // X-Forwarded-For has priority
        assert_eq!(ip, Some("203.0.113.1".to_string()));
    }

    #[test]
    fn test_no_ip_headers() {
        let headers = HeaderMap::new();
        let ip = extract_client_ip(&headers);
        assert_eq!(ip, None);
    }

    #[test]
    fn test_empty_x_forwarded_for() {
        let mut headers = HeaderMap::new();
        headers.insert(
            "x-forwarded-for",
            "".parse().expect("Invalid IP address in test"),
        );

        let ip = extract_client_ip(&headers);
        assert_eq!(ip, None);
    }

    #[test]
    fn test_whitespace_handling() {
        let mut headers = HeaderMap::new();
        headers.insert(
            "x-forwarded-for",
            "  203.0.113.1  , 10.0.0.1"
                .parse()
                .expect("Invalid IP address in test"),
        );

        let ip = extract_client_ip(&headers);
        assert_eq!(ip, Some("203.0.113.1".to_string()));
    }

    #[test]
    fn test_ipv6_address() {
        let mut headers = HeaderMap::new();
        headers.insert(
            "x-forwarded-for",
            "2001:db8::1, 192.168.1.1"
                .parse()
                .expect("Invalid IP address in test"),
        );

        let ip = extract_client_ip(&headers);
        assert_eq!(ip, Some("2001:db8::1".to_string()));
    }
}
