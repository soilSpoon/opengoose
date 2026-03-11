use std::net::{IpAddr, Ipv4Addr, SocketAddr};

use anyhow::{Context, Result};
use opengoose_web::{WebOptions, serve};

const DEFAULT_WEB_HOST: IpAddr = IpAddr::V4(Ipv4Addr::LOCALHOST);
const DEFAULT_WEB_PORT: u16 = 8080;
const HOST_ENV_VAR: &str = "OPENGOOSE_HOST";
const PORT_ENV_VAR: &str = "OPENGOOSE_PORT";

/// Run the web dashboard server.
pub async fn execute(
    host: Option<IpAddr>,
    port: Option<u16>,
    tls_cert: Option<std::path::PathBuf>,
    tls_key: Option<std::path::PathBuf>,
) -> Result<()> {
    let bind = resolve_bind_addr(host, port)?;
    serve(WebOptions {
        bind,
        tls_cert_path: tls_cert,
        tls_key_path: tls_key,
    })
    .await
}

fn resolve_bind_addr(host: Option<IpAddr>, port: Option<u16>) -> Result<SocketAddr> {
    Ok(SocketAddr::from((resolve_host(host)?, resolve_port(port)?)))
}

fn resolve_host(host: Option<IpAddr>) -> Result<IpAddr> {
    match host {
        Some(host) => Ok(host),
        None => resolve_host_from_env(),
    }
}

fn resolve_port(port: Option<u16>) -> Result<u16> {
    match port {
        Some(port) => Ok(port),
        None => resolve_port_from_env(),
    }
}

fn resolve_host_from_env() -> Result<IpAddr> {
    let Some(host) = std::env::var_os(HOST_ENV_VAR) else {
        return Ok(DEFAULT_WEB_HOST);
    };

    let host = host.to_string_lossy();
    let trimmed = host.trim();
    if trimmed.is_empty() {
        return Ok(DEFAULT_WEB_HOST);
    }

    trimmed.parse().with_context(|| {
        format!("invalid {HOST_ENV_VAR} value `{trimmed}`: expected an IP address")
    })
}

fn resolve_port_from_env() -> Result<u16> {
    let Some(port) = std::env::var_os(PORT_ENV_VAR) else {
        return Ok(DEFAULT_WEB_PORT);
    };

    let port = port.to_string_lossy();
    let trimmed = port.trim();
    if trimmed.is_empty() {
        return Ok(DEFAULT_WEB_PORT);
    }

    trimmed.parse().with_context(|| {
        format!("invalid {PORT_ENV_VAR} value `{trimmed}`: expected a port number")
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Mutex;

    static ENV_LOCK: Mutex<()> = Mutex::new(());

    fn with_web_env<T>(host: Option<&str>, port: Option<&str>, test: impl FnOnce() -> T) -> T {
        let _guard = ENV_LOCK.lock().unwrap();
        let saved_host = std::env::var(HOST_ENV_VAR).ok();
        let saved_port = std::env::var(PORT_ENV_VAR).ok();

        unsafe {
            match host {
                Some(value) => std::env::set_var(HOST_ENV_VAR, value),
                None => std::env::remove_var(HOST_ENV_VAR),
            }
            match port {
                Some(value) => std::env::set_var(PORT_ENV_VAR, value),
                None => std::env::remove_var(PORT_ENV_VAR),
            }
        }

        let result = test();

        unsafe {
            match saved_host {
                Some(value) => std::env::set_var(HOST_ENV_VAR, value),
                None => std::env::remove_var(HOST_ENV_VAR),
            }
            match saved_port {
                Some(value) => std::env::set_var(PORT_ENV_VAR, value),
                None => std::env::remove_var(PORT_ENV_VAR),
            }
        }

        result
    }

    #[test]
    fn resolve_bind_addr_defaults_to_localhost_8080() {
        with_web_env(None, None, || {
            let bind = resolve_bind_addr(None, None).unwrap();
            assert_eq!(bind, SocketAddr::from((Ipv4Addr::LOCALHOST, 8080)));
        });
    }

    #[test]
    fn resolve_bind_addr_uses_env_when_flags_are_absent() {
        with_web_env(Some("0.0.0.0"), Some("9090"), || {
            let bind = resolve_bind_addr(None, None).unwrap();
            assert_eq!(bind, SocketAddr::from(([0, 0, 0, 0], 9090)));
        });
    }

    #[test]
    fn resolve_bind_addr_prefers_cli_flags_over_env() {
        with_web_env(Some("127.0.0.1"), Some("9090"), || {
            let bind = resolve_bind_addr(Some("0.0.0.0".parse().unwrap()), Some(8081)).unwrap();
            assert_eq!(bind, SocketAddr::from(([0, 0, 0, 0], 8081)));
        });
    }

    #[test]
    fn resolve_bind_addr_rejects_invalid_host_env() {
        with_web_env(Some("not-an-ip"), None, || {
            let err = resolve_bind_addr(None, None).unwrap_err();
            assert!(
                err.to_string().contains("invalid OPENGOOSE_HOST value"),
                "unexpected error: {err}"
            );
        });
    }

    #[test]
    fn resolve_bind_addr_rejects_invalid_port_env() {
        with_web_env(None, Some("abc"), || {
            let err = resolve_bind_addr(None, None).unwrap_err();
            assert!(
                err.to_string().contains("invalid OPENGOOSE_PORT value"),
                "unexpected error: {err}"
            );
        });
    }
}
