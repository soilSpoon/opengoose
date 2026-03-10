use anyhow::Result;
use opengoose_web::{WebOptions, serve};

/// Run the web dashboard server.
pub async fn execute(port: u16) -> Result<()> {
    serve(WebOptions {
        bind: ([127, 0, 0, 1], port).into(),
    })
    .await
}
