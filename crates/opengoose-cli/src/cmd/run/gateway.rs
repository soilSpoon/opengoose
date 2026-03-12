use std::sync::Arc;

use goose::gateway::Gateway;
use opengoose_core::{Engine, GatewayBridge};
use opengoose_discord::DiscordGateway;
use opengoose_matrix::MatrixGateway;
use opengoose_secrets::{CredentialResolver, SecretKey};
use opengoose_slack::SlackGateway;
use opengoose_telegram::TelegramGateway;
use opengoose_types::EventBus;

type GatewayBuilder = fn(&[&str], Arc<GatewayBridge>, EventBus) -> anyhow::Result<Arc<dyn Gateway>>;

/// Declarative specification for constructing a gateway from credentials.
///
/// To add a new channel, add a single entry to [`gateway_specs`] — no other
/// changes needed in this file.
pub(super) struct GatewaySpec {
    /// Secret keys that must all resolve (order matches `build` parameter order).
    pub keys: Vec<SecretKey>,
    /// Construct the gateway from resolved credential values, a bridge, and the event bus.
    pub build: GatewayBuilder,
}

/// Registry of all supported gateway specifications.
pub(super) fn gateway_specs() -> Vec<GatewaySpec> {
    vec![
        GatewaySpec {
            keys: vec![SecretKey::DiscordBotToken],
            build: |creds, bridge, bus| Ok(Arc::new(DiscordGateway::new(creds[0], bridge, bus))),
        },
        GatewaySpec {
            keys: vec![SecretKey::TelegramBotToken],
            build: |creds, bridge, bus| Ok(Arc::new(TelegramGateway::new(creds[0], bridge, bus)?)),
        },
        GatewaySpec {
            keys: vec![SecretKey::SlackAppToken, SecretKey::SlackBotToken],
            build: |creds, bridge, bus| {
                Ok(Arc::new(SlackGateway::new(creds[0], creds[1], bridge, bus)))
            },
        },
        GatewaySpec {
            keys: vec![SecretKey::MatrixHomeserverUrl, SecretKey::MatrixAccessToken],
            build: |creds, bridge, bus| {
                Ok(Arc::new(MatrixGateway::new(
                    creds[0], creds[1], bridge, bus,
                )?))
            },
        },
    ]
}

/// Collect all gateways for which credentials are available.
pub(super) async fn collect_gateways(
    resolver: &CredentialResolver,
    engine: Arc<Engine>,
    event_bus: &EventBus,
) -> (Vec<Arc<dyn Gateway>>, Vec<Arc<GatewayBridge>>) {
    let mut gateways: Vec<Arc<dyn Gateway>> = vec![];
    let mut bridges: Vec<Arc<GatewayBridge>> = vec![];

    for spec in gateway_specs() {
        // Resolve all required credentials; skip this gateway if any are missing.
        let mut values = Vec::with_capacity(spec.keys.len());
        let mut all_resolved = true;
        for key in &spec.keys {
            match resolver.resolve_async(key).await {
                Ok(cred) => values.push(cred.value),
                Err(_) => {
                    all_resolved = false;
                    break;
                }
            }
        }
        if !all_resolved {
            continue;
        }

        let bridge = Arc::new(GatewayBridge::new(engine.clone()));
        let cred_strs: Vec<&str> = values.iter().map(|v| v.as_str()).collect();
        match (spec.build)(&cred_strs, bridge.clone(), event_bus.clone()) {
            Ok(gw) => {
                gateways.push(gw);
                bridges.push(bridge);
            }
            Err(e) => {
                tracing::warn!("failed to create gateway: {e}");
            }
        }
    }

    (gateways, bridges)
}
