use axum::http::HeaderMap;

pub(crate) fn websocket_url(headers: &HeaderMap) -> String {
    let host = forwarded_header(headers, "x-forwarded-host")
        .or_else(|| forwarded_host(headers))
        .or_else(|| header_string(headers, "host"))
        .unwrap_or_else(|| "localhost:3000".into());
    let scheme = match forwarded_header(headers, "x-forwarded-proto")
        .or_else(|| forwarded_proto(headers))
        .as_deref()
    {
        Some("https") | Some("wss") => "wss",
        _ => "ws",
    };

    format!("{scheme}://{host}/api/agents/connect")
}

fn header_string(headers: &HeaderMap, name: &str) -> Option<String> {
    headers
        .get(name)
        .and_then(|value| value.to_str().ok())
        .map(|value| value.split(',').next().unwrap_or(value).trim().to_string())
        .filter(|value| !value.is_empty())
}

fn forwarded_header(headers: &HeaderMap, name: &str) -> Option<String> {
    header_string(headers, name)
}

fn forwarded_proto(headers: &HeaderMap) -> Option<String> {
    headers
        .get("forwarded")
        .and_then(|value| value.to_str().ok())
        .and_then(|value| {
            value
                .split(';')
                .find_map(|segment| segment.trim().strip_prefix("proto="))
        })
        .map(|value| value.trim_matches('"').to_string())
}

fn forwarded_host(headers: &HeaderMap) -> Option<String> {
    headers
        .get("forwarded")
        .and_then(|value| value.to_str().ok())
        .and_then(|value| {
            value
                .split(';')
                .find_map(|segment| segment.trim().strip_prefix("host="))
        })
        .map(|value| value.trim_matches('"').to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn websocket_url_prefers_forwarded_https_headers() {
        let mut headers = HeaderMap::new();
        headers.insert(
            "x-forwarded-host",
            "goose.example.com".parse().expect("forwarded host"),
        );
        headers.insert(
            "x-forwarded-proto",
            "https".parse().expect("forwarded proto"),
        );
        headers.insert("host", "localhost:3000".parse().expect("host header"));

        assert_eq!(
            websocket_url(&headers),
            "wss://goose.example.com/api/agents/connect"
        );
    }

    #[test]
    fn websocket_url_uses_forwarded_header_when_proxy_headers_are_absent() {
        let mut headers = HeaderMap::new();
        headers.insert(
            "forwarded",
            "for=192.0.2.10;host=\"proxy.example.com\";proto=https"
                .parse()
                .expect("forwarded header"),
        );

        assert_eq!(
            websocket_url(&headers),
            "wss://proxy.example.com/api/agents/connect"
        );
    }

    #[test]
    fn websocket_url_falls_back_to_host_header_and_ws_scheme() {
        let mut headers = HeaderMap::new();
        headers.insert("host", "opengoose.test".parse().expect("host header"));

        assert_eq!(
            websocket_url(&headers),
            "ws://opengoose.test/api/agents/connect"
        );
    }
}
