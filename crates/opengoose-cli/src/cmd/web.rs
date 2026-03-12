use crate::error::{CliError, CliResult};
use opengoose_web::{WebOptions, serve};

/// Run the web dashboard server.
pub async fn execute(
    port: u16,
    tls_cert: Option<std::path::PathBuf>,
    tls_key: Option<std::path::PathBuf>,
) -> CliResult<()> {
    serve(WebOptions {
        bind: ([127, 0, 0, 1], port).into(),
        tls_cert_path: tls_cert,
        tls_key_path: tls_key,
    })
    .await
    .map_err(CliError::Other)?;
    Ok(())
}
