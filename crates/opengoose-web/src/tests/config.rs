//! Tests for `WebOptions` configuration and TLS path handling.

#[test]
fn web_options_plain_has_no_tls() {
    use std::net::{Ipv4Addr, SocketAddr};
    let addr = SocketAddr::from((Ipv4Addr::LOCALHOST, 8080));
    let opts = crate::WebOptions::plain(addr);
    assert_eq!(opts.bind, addr);
    assert!(opts.tls_cert_path.is_none());
    assert!(opts.tls_key_path.is_none());
}

#[test]
fn web_options_default_has_no_tls() {
    let opts = crate::WebOptions::default();
    assert!(opts.tls_cert_path.is_none());
    assert!(opts.tls_key_path.is_none());
}

#[test]
fn web_options_with_tls_paths_set() {
    use std::net::{Ipv4Addr, SocketAddr};
    let opts = crate::WebOptions {
        bind: SocketAddr::from((Ipv4Addr::LOCALHOST, 8443)),
        tls_cert_path: Some("/etc/ssl/cert.pem".into()),
        tls_key_path: Some("/etc/ssl/key.pem".into()),
    };
    assert_eq!(
        opts.tls_cert_path.unwrap().to_str().unwrap(),
        "/etc/ssl/cert.pem"
    );
    assert_eq!(
        opts.tls_key_path.unwrap().to_str().unwrap(),
        "/etc/ssl/key.pem"
    );
}

#[test]
fn web_options_is_clone() {
    use std::net::{Ipv4Addr, SocketAddr};
    let opts = crate::WebOptions {
        bind: SocketAddr::from((Ipv4Addr::LOCALHOST, 9443)),
        tls_cert_path: Some("/cert.pem".into()),
        tls_key_path: Some("/key.pem".into()),
    };
    let cloned = opts.clone();
    assert_eq!(cloned.tls_cert_path, opts.tls_cert_path);
    assert_eq!(cloned.tls_key_path, opts.tls_key_path);
}
