//! URL builder that omits default scheme ports (80/443).

pub fn build_url(scheme: &str, host: &str, port: u16, path: &str) -> String {
    let omit_port = ((scheme == "http" || scheme == "ws") && port == 80)
        || ((scheme == "https" || scheme == "wss") && port == 443);
    let port_part = if omit_port {
        String::new()
    } else {
        format!(":{port}")
    };
    format!("{scheme}://{host}{port_part}{path}")
}

#[cfg(test)]
mod tests {
    use super::build_url;

    #[test]
    fn omits_default_http_and_ws_port() {
        assert_eq!(
            build_url("http", "192.168.1.5", 80, ""),
            "http://192.168.1.5"
        );
        assert_eq!(
            build_url("ws", "192.168.1.5", 80, "/status"),
            "ws://192.168.1.5/status"
        );
    }

    #[test]
    fn omits_default_https_and_wss_port() {
        assert_eq!(
            build_url("https", "192.168.1.5", 443, "/fs"),
            "https://192.168.1.5/fs"
        );
        assert_eq!(
            build_url("wss", "192.168.1.5", 443, "/status"),
            "wss://192.168.1.5/status"
        );
    }

    #[test]
    fn keeps_non_default_ports() {
        assert_eq!(
            build_url("https", "192.168.1.5", 8443, "/peer_graphql"),
            "https://192.168.1.5:8443/peer_graphql"
        );
        assert_eq!(
            build_url("http", "localhost", 8080, ""),
            "http://localhost:8080"
        );
    }

    #[test]
    fn keeps_default_port_under_other_scheme() {
        assert_eq!(
            build_url("https", "192.168.1.5", 80, "/fs"),
            "https://192.168.1.5:80/fs"
        );
        assert_eq!(
            build_url("http", "192.168.1.5", 443, ""),
            "http://192.168.1.5:443"
        );
    }
}
