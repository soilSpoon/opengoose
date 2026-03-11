use tokio::sync::oneshot::error::TryRecvError;

use super::super::state::*;

pub(super) fn poll(app: &mut App) {
    poll_provider_loading(app);
    poll_model_loading(app);
    poll_oauth_completion(app);
}

fn poll_provider_loading(app: &mut App) {
    let provider_result = app.provider_loading_rx.as_mut().map(|rx| rx.try_recv());

    match provider_result {
        Some(Ok(providers)) => {
            app.cached_providers = providers;
            app.provider_loading_rx = None;
            app.populate_provider_select_from_cache();
        }
        Some(Err(TryRecvError::Closed)) => {
            app.provider_loading_rx = None;
            app.push_event("Failed to load providers.", EventLevel::Error);
            app.set_status_notice(
                "Provider list could not be loaded. Check your connection and retry.".to_string(),
                EventLevel::Error,
            );
            app.provider_select.visible = false;
        }
        Some(Err(TryRecvError::Empty)) | None => {}
    }
}

fn poll_model_loading(app: &mut App) {
    let model_result = app.model_loading_rx.as_mut().map(|rx| rx.try_recv());

    match model_result {
        Some(Ok(models)) => {
            app.model_select.models = models;
            app.model_select.loading = false;
            app.model_loading_rx = None;
        }
        Some(Err(TryRecvError::Closed)) => {
            app.model_loading_rx = None;
            app.model_select.loading = false;
            app.push_event("Failed to fetch models.", EventLevel::Error);
            app.set_status_notice(
                "Model lookup failed. The provider may be unavailable right now.".to_string(),
                EventLevel::Error,
            );
        }
        Some(Err(TryRecvError::Empty)) | None => {}
    }
}

fn poll_oauth_completion(app: &mut App) {
    let oauth_result = match app.oauth_done_rx.as_mut() {
        Some(rx) => match rx.try_recv() {
            Ok(result) => Some(result),
            Err(TryRecvError::Closed) => {
                Some(Err(anyhow::anyhow!("OAuth task terminated unexpectedly")))
            }
            Err(TryRecvError::Empty) => None,
        },
        None => None,
    };

    let Some(result) = oauth_result else {
        return;
    };

    app.oauth_done_rx = None;

    match result {
        Ok(()) => handle_oauth_success(app),
        Err(error) => handle_oauth_failure(app, error),
    }
}

fn handle_oauth_success(app: &mut App) {
    app.push_event(
        &format!(
            "OAuth completed for {}.",
            app.credential_flow
                .provider_display
                .as_deref()
                .unwrap_or("")
        ),
        EventLevel::Info,
    );

    if app.credential_flow.has_more() {
        app.credential_flow.current_key += 1;
        app.advance_credential_flow();
        return;
    }

    if let Err(error) = app.store_credentials() {
        let message = format!("Failed to store credentials: {error}");
        app.push_event(&message, EventLevel::Error);
        app.set_status_notice(message, EventLevel::Error);
        app.credential_flow.reset();
    }
}

fn handle_oauth_failure(app: &mut App, error: anyhow::Error) {
    let message = format!("OAuth failed: {error}");
    app.push_event(&message, EventLevel::Error);
    app.set_status_notice(message, EventLevel::Error);
    app.credential_flow.reset();
}
