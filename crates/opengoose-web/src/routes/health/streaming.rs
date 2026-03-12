use opengoose_types::AppEventKind;

use crate::data::{StatusPageView, load_status_page};
use crate::server::PageState;

pub(super) fn load_status_page_view(state: &PageState) -> anyhow::Result<StatusPageView> {
    load_status_page(state.db.clone(), state.channel_metrics.clone())
}

pub(super) fn matches_status_live_event(kind: &AppEventKind) -> bool {
    matches!(
        kind,
        AppEventKind::DashboardUpdated
            | AppEventKind::ChannelReady { .. }
            | AppEventKind::ChannelDisconnected { .. }
            | AppEventKind::ChannelReconnecting { .. }
            | AppEventKind::AlertFired { .. }
            | AppEventKind::QueueUpdated { .. }
            | AppEventKind::RunUpdated { .. }
            | AppEventKind::TeamRunStarted { .. }
            | AppEventKind::TeamStepStarted { .. }
            | AppEventKind::TeamStepCompleted { .. }
            | AppEventKind::TeamStepFailed { .. }
            | AppEventKind::TeamRunCompleted { .. }
            | AppEventKind::TeamRunFailed { .. }
    )
}

pub(super) fn status_stream_error_html() -> &'static str {
    r#"
<section id="status-page-intro" class="hero-panel">
  <div class="hero-copy">
    <p class="eyebrow">System status</p>
    <h1>Status snapshot unavailable.</h1>
    <p class="hero-text">The health board will keep listening for runtime events while a slower fallback sweep stays armed.</p>
  </div>
  <div class="hero-status">
    <p class="eyebrow">Live transport</p>
    <div class="live-chip-row">
      <span class="chip tone-rose">Stream degraded</span>
    </div>
    <p>Retrying automatically when the next event or fallback sweep lands.</p>
  </div>
</section>
<div id="status-live">
  <section class="callout tone-danger">
    <p class="eyebrow">Status unavailable</p>
    <h2>Live health probe failed</h2>
    <p>The board keeps listening for fresh runtime signals and will re-render on the next fallback sweep if the stream stays quiet.</p>
  </section>
</div>
"#
}
