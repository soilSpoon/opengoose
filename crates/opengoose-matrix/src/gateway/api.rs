use std::sync::atomic::Ordering;

use crate::types::{MatrixError, SendEventResponse, SyncFilter, SyncResponse, WhoAmI};

use super::{MatrixGateway, SYNC_TIMEOUT_MS, urlencoding};

impl MatrixGateway {
    pub(super) fn v3_url(&self, path: &str) -> String {
        format!("{}/_matrix/client/v3{}", self.homeserver_url, path)
    }

    pub(super) fn next_txn_id(&self) -> String {
        let n = self.txn_counter.fetch_add(1, Ordering::Relaxed);
        format!("opengoose-{}-{}", std::process::id(), n)
    }

    pub(super) fn auth_header(&self) -> String {
        format!("Bearer {}", self.access_token)
    }

    /// GET /account/whoami — returns the bot's Matrix user ID.
    pub(super) async fn whoami(&self) -> anyhow::Result<String> {
        let resp: WhoAmI = self
            .client
            .get(self.v3_url("/account/whoami"))
            .header("Authorization", self.auth_header())
            .send()
            .await?
            .json()
            .await?;
        Ok(resp.user_id)
    }

    /// Register a minimal sync filter and return the filter ID.
    pub(super) async fn register_filter(&self, user_id: &str) -> anyhow::Result<String> {
        let encoded_user = urlencoding::encode(user_id).into_owned();
        let filter = SyncFilter::messages_only();
        let resp: serde_json::Value = self
            .client
            .post(self.v3_url(&format!("/user/{encoded_user}/filter")))
            .header("Authorization", self.auth_header())
            .json(&filter)
            .send()
            .await?
            .json()
            .await?;
        resp.get("filter_id")
            .and_then(|v| v.as_str())
            .map(|s| s.to_string())
            .ok_or_else(|| anyhow::anyhow!("no filter_id in response"))
    }

    /// GET /sync — long-poll for new events.
    pub(super) async fn sync(
        &self,
        since: Option<&str>,
        filter_id: Option<&str>,
    ) -> anyhow::Result<SyncResponse> {
        let mut req = self
            .client
            .get(self.v3_url("/sync"))
            .header("Authorization", self.auth_header())
            .query(&[("timeout", SYNC_TIMEOUT_MS.to_string())]);

        if let Some(s) = since {
            req = req.query(&[("since", s)]);
        }
        if let Some(f) = filter_id {
            req = req.query(&[("filter", f)]);
        }

        Ok(req.send().await?.json().await?)
    }

    /// PUT /rooms/{roomId}/send/{eventType}/{txnId} — send a message event.
    pub(super) async fn send_event(
        &self,
        room_id: &str,
        content: &serde_json::Value,
    ) -> anyhow::Result<String> {
        let encoded_room = urlencoding::encode(room_id).into_owned();
        let txn_id = self.next_txn_id();
        let url = self.v3_url(&format!(
            "/rooms/{encoded_room}/send/m.room.message/{txn_id}"
        ));

        let resp = self
            .client
            .put(&url)
            .header("Authorization", self.auth_header())
            .json(content)
            .send()
            .await?;

        if !resp.status().is_success() {
            let err: MatrixError = resp.json().await.unwrap_or(MatrixError {
                errcode: None,
                error: Some("unknown error".into()),
            });
            anyhow::bail!(
                "send_event failed: {} — {}",
                err.errcode.unwrap_or_default(),
                err.error.unwrap_or_default()
            );
        }

        let ev: SendEventResponse = resp.json().await?;
        Ok(ev.event_id)
    }
}
