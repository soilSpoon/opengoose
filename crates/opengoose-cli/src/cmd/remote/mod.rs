use anyhow::Result;
use clap::Subcommand;

mod connect;
mod http;
mod protocol;

#[cfg(test)]
mod tests;

/// Default base URL for the OpenGoose web server.
const DEFAULT_BASE: &str = "http://127.0.0.1:8080";

#[derive(Subcommand)]
/// Subcommands for `opengoose remote`.
pub enum RemoteAction {
    /// Connect to an OpenGoose server as a remote agent
    Connect {
        /// WebSocket URL of the OpenGoose server (e.g. ws://localhost:8080)
        url: String,
        /// API key for authentication
        #[arg(long)]
        key: Option<String>,
        /// Agent name to register as
        #[arg(long)]
        name: String,
    },
    /// List connected remote agents
    List {
        /// Base URL of the web server (default: http://127.0.0.1:8080)
        #[arg(long, default_value = DEFAULT_BASE)]
        url: String,
    },
    /// Disconnect a remote agent by name
    Disconnect {
        /// Name of the remote agent to disconnect
        name: String,
        /// Base URL of the web server (default: http://127.0.0.1:8080)
        #[arg(long, default_value = DEFAULT_BASE)]
        url: String,
    },
}

/// Dispatch and execute the selected remote subcommand.
pub async fn execute(action: RemoteAction) -> Result<()> {
    match action {
        RemoteAction::Connect { url, key, name } => {
            connect::cmd_connect(&url, key.as_deref(), &name).await
        }
        RemoteAction::List { url } => http::cmd_list(&url).await,
        RemoteAction::Disconnect { name, url } => http::cmd_disconnect(&name, &url).await,
    }
}
