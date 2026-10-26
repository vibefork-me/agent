//! Custom AI endpoint validation and host formatting.
//! Layer 0: pure data and validation logic, no IO.

/// Checks if an endpoint URL is valid and allowed:
/// - Must begin with `http://` or `https://`.
/// - `https://` is permitted for all hosts.
/// - `http://` is permitted ONLY for local loopback addresses (`localhost`, `127.0.0.1`, `::1`).
///
/// Returns `Ok(normalized_url)` on success, or `Err(&str)` with the reason.
pub fn validate_custom_openai_endpoint(raw: &str) -> Result<String, &'static str> {
    let s = raw.trim();
    if s.is_empty() {
        return Err("Endpoint URL cannot be empty");
    }
    let (is_http, rest) = if let Some(rest) = s.strip_prefix("https://") {
        (false, rest)
    } else if let Some(rest) = s.strip_prefix("http://") {
        (true, rest)
    } else {
        return Err("Endpoint URL must start with https:// or http://");
    };

    if rest.is_empty() {
        return Err("Endpoint URL is missing host");
    }

    // Extract host part (up to '/', ':', or end)
    let host_and_port = rest.split('/').next().unwrap_or_default();
    if host_and_port.is_empty() {
        return Err("Endpoint URL is missing host");
    }

    let host = if host_and_port.starts_with('[') {
        // IPv6 bracketed address e.g. [::1]:11434
        if let Some(end_bracket) = host_and_port.find(']') {
            &host_and_port[1..end_bracket]
        } else {
            return Err("Invalid IPv6 host syntax");
        }
    } else {
        host_and_port.split(':').next().unwrap_or_default()
    };

    let host_lower = host.to_ascii_lowercase();
    let is_loopback = is_loopback_host(&host_lower);

    if is_http && !is_loopback {
        return Err(
            "HTTP is only allowed for local loopback addresses (localhost, 127.0.0.1). For remote servers, HTTPS is required.",
        );
    }

    // Return normalized without trailing slashes
    Ok(s.trim_end_matches('/').to_owned())
}

/// Determines if a hostname/IP represents a local loopback address.
fn is_loopback_host(host: &str) -> bool {
    if host == "localhost" || host == "::1" {
        return true;
    }
    // Check for 127.0.0.0/8
    if let Ok(ip) = host.parse::<std::net::Ipv4Addr>() {
        return ip.is_loopback();
    }
    if let Ok(ip) = host.parse::<std::net::Ipv6Addr>() {
        return ip.is_loopback();
    }
    false
}

/// Extracts a concise host label from an endpoint URL (e.g. "localhost:11434" or "api.together.xyz").
pub fn endpoint_host_label(endpoint: &str) -> String {
    let after_scheme = if let Some(rest) = endpoint.strip_prefix("https://") {
        rest
    } else if let Some(rest) = endpoint.strip_prefix("http://") {
        rest
    } else {
        endpoint
    };
    after_scheme
        .split('/')
        .next()
        .unwrap_or(endpoint)
        .to_owned()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn custom_openai_endpoint_validation_and_host_label() {
        // Valid loopback http
        assert_eq!(
            validate_custom_openai_endpoint("http://localhost:11434/v1/"),
            Ok("http://localhost:11434/v1".to_owned())
        );
        assert_eq!(
            validate_custom_openai_endpoint("http://127.0.0.1:1234/v1"),
            Ok("http://127.0.0.1:1234/v1".to_owned())
        );
        assert_eq!(
            validate_custom_openai_endpoint("http://[::1]:11434"),
            Ok("http://[::1]:11434".to_owned())
        );

        // Valid remote https
        assert_eq!(
            validate_custom_openai_endpoint("https://api.together.xyz/v1/"),
            Ok("https://api.together.xyz/v1".to_owned())
        );
        assert_eq!(
            validate_custom_openai_endpoint("https://api.openai.com/v1"),
            Ok("https://api.openai.com/v1".to_owned())
        );

        // Invalid: non-loopback http rejected
        assert!(validate_custom_openai_endpoint("http://api.together.xyz/v1").is_err());
        assert!(validate_custom_openai_endpoint("http://192.168.1.50:11434").is_err());
        assert!(validate_custom_openai_endpoint("http://example.com").is_err());

        // Invalid scheme or missing host
        assert!(validate_custom_openai_endpoint("ftp://localhost:11434").is_err());
        assert!(validate_custom_openai_endpoint("localhost:11434").is_err());
        assert!(validate_custom_openai_endpoint("https://").is_err());

        // Host label
        assert_eq!(
            endpoint_host_label("http://localhost:11434/v1"),
            "localhost:11434"
        );
        assert_eq!(
            endpoint_host_label("https://api.together.xyz/v1/chat"),
            "api.together.xyz"
        );
        assert_eq!(endpoint_host_label("http://[::1]:8080/v1"), "[::1]:8080");
    }
}
