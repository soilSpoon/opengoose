use std::sync::Arc;

use opengoose_core::GatewayBridge;
use tokio_util::sync::CancellationToken;

/// Spawn a task that listens for pairing code generation requests.
///
/// Generates a pairing code on ALL bridges so that any connected channel
/// can serve the pairing flow — not just the first gateway.
pub(super) async fn for_each_pairing_target<T, F, Fut>(
    targets: &[T],
    platforms: &[String],
    mut f: F,
) where
    T: Clone,
    F: FnMut(T, String) -> Fut,
    Fut: std::future::Future<Output = ()>,
{
    for (target, platform) in targets.iter().zip(platforms.iter()) {
        f(target.clone(), platform.clone()).await;
    }
}

pub(super) async fn generate_pairing_codes(bridges: &[Arc<GatewayBridge>], platforms: &[String]) {
    for_each_pairing_target(bridges, platforms, |bridge, platform| async move {
        if let Err(e) = bridge.generate_pairing_code(&platform).await {
            tracing::error!(%e, %platform, "failed to generate pairing code");
        }
    })
    .await;
}

#[cfg(test)]
pub(super) async fn record_pairing_platforms(
    targets: &[String],
    platforms: &[String],
) -> Vec<String> {
    let seen = std::sync::Arc::new(std::sync::Mutex::new(Vec::new()));
    for_each_pairing_target(targets, platforms, {
        let seen = seen.clone();
        move |_target, platform| {
            let seen = seen.clone();
            async move {
                seen.lock().unwrap().push(platform);
            }
        }
    })
    .await;
    seen.lock().unwrap().clone()
}

#[cfg(test)]
pub(super) async fn record_pairing_pairs(
    targets: &[String],
    platforms: &[String],
) -> Vec<(String, String)> {
    let seen = std::sync::Arc::new(std::sync::Mutex::new(Vec::new()));
    for_each_pairing_target(targets, platforms, {
        let seen = seen.clone();
        move |target, platform| {
            let seen = seen.clone();
            async move {
                seen.lock().unwrap().push((target, platform));
            }
        }
    })
    .await;
    seen.lock().unwrap().clone()
}

#[cfg(test)]
pub(super) fn test_pairing_targets(names: &[&str]) -> Vec<String> {
    names.iter().map(|name| (*name).to_string()).collect()
}

pub(super) fn spawn_pairing_handler(
    bridges: Vec<Arc<GatewayBridge>>,
    platforms: Vec<String>,
    mut rx: tokio::sync::mpsc::UnboundedReceiver<()>,
    cancel: CancellationToken,
) {
    tokio::spawn(async move {
        loop {
            tokio::select! {
                _ = cancel.cancelled() => break,
                req = rx.recv() => {
                    match req {
                        Some(()) => generate_pairing_codes(&bridges, &platforms).await,
                        None => break,
                    }
                }
            }
        }
    });
}
