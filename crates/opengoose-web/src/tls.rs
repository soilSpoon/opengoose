use std::net::SocketAddr;

use axum::Router;
use tracing::info;

use crate::server::WebOptions;

/// Start the server with TLS or plain HTTP based on the provided options.
pub(crate) async fn start_server(options: WebOptions, app: Router) -> anyhow::Result<()> {
    match (options.tls_cert_path, options.tls_key_path) {
        (Some(cert), Some(key)) => {
            let tls_config = axum_server::tls_rustls::RustlsConfig::from_pem_file(&cert, &key)
                .await
                .map_err(|e| {
                    anyhow::anyhow!(
                        "failed to load TLS config (cert={}, key={}): {}",
                        cert.display(),
                        key.display(),
                        e
                    )
                })?;
            info!(address = %options.bind, "serving opengoose web dashboard (TLS/WSS)");
            axum_server::bind_rustls(options.bind, tls_config)
                .serve(app.into_make_service_with_connect_info::<SocketAddr>())
                .await?;
        }
        (Some(_), None) => {
            anyhow::bail!("--tls-cert requires --tls-key to also be provided");
        }
        (None, Some(_)) => {
            anyhow::bail!("--tls-key requires --tls-cert to also be provided");
        }
        (None, None) => {
            let listener = tokio::net::TcpListener::bind(options.bind).await?;
            info!(address = %options.bind, "serving opengoose web dashboard");
            axum::serve(
                listener,
                app.into_make_service_with_connect_info::<SocketAddr>(),
            )
            .await?;
        }
    }
    Ok(())
}
