import { initDashboardStreams } from "./dashboard-stream.js";
import { initListShells } from "./list-shell.js";
import { initRemoteAgentActions } from "./remote-agents.js";
import { initTableShells } from "./table-shell.js";
import { initWorkflowTriggers } from "./workflow-trigger.js";

const connectionTones = {
  connecting: "amber",
  live: "success",
  retrying: "amber",
  degraded: "rose",
};

const connectionLabels = {
  connecting: "Connecting",
  live: "SSE live",
  retrying: "Reconnecting",
  degraded: "Stream degraded",
};

const liveEventState = new WeakMap();

const parseCsv = (value) =>
  `${value || ""}`
    .split(",")
    .map((item) => item.trim())
    .filter(Boolean);

const setConnectionStatus = (owner, state) => {
  const chip =
    owner?.querySelector("[data-live-events-connection]") ||
    owner?.querySelector("[data-dashboard-connection]");
  if (!chip) return;

  chip.textContent = connectionLabels[state] || connectionLabels.connecting;
  chip.className = `chip tone-${connectionTones[state] || connectionTones.connecting}`;
};

const refreshFragments = async (owner, selectors) => {
  const response = await fetch(window.location.href, {
    cache: "no-store",
    headers: {
      "X-Requested-With": "opengoose-live-events",
    },
  });
  if (!response.ok) {
    throw new Error(`live refresh failed with ${response.status}`);
  }

  const html = await response.text();
  const nextDocument = new DOMParser().parseFromString(html, "text/html");

  selectors.forEach((selector) => {
    const current = document.querySelector(selector);
    const replacement = nextDocument.querySelector(selector);
    if (!current || !replacement) return;
    current.replaceWith(replacement);
  });

  initListShells(document);
  initTableShells(document);
  initDashboardStreams(document);
  initRemoteAgentActions(document);
  initWorkflowTriggers(document);
  setConnectionStatus(owner, "live");
};

const scheduleRefresh = (owner) => {
  const state = liveEventState.get(owner);
  if (!state || state.refreshing) return;

  if (state.pendingTimer) {
    window.clearTimeout(state.pendingTimer);
  }

  state.pendingTimer = window.setTimeout(async () => {
    state.pendingTimer = null;
    state.refreshing = true;

    try {
      await refreshFragments(owner, state.selectors);
    } catch (_error) {
      setConnectionStatus(owner, "degraded");
    } finally {
      state.refreshing = false;
    }
  }, 180);
};

const bindLiveEvents = (owner) => {
  if (!owner || owner.dataset.liveEventsBound === "true") return;

  const eventTypes = parseCsv(owner.dataset.liveEventsTypes);
  const selectors = parseCsv(owner.dataset.liveEventsTargets);
  if (selectors.length === 0) return;

  owner.dataset.liveEventsBound = "true";

  const state = {
    selectors,
    pendingTimer: null,
    refreshing: false,
  };
  liveEventState.set(owner, state);

  const url =
    owner.dataset.liveEventsUrl ||
    `/api/events${eventTypes.length ? `?types=${eventTypes.join(",")}` : ""}`;

  const source = new EventSource(url);

  source.onopen = () => {
    setConnectionStatus(owner, "live");
  };

  source.onerror = () => {
    if (source.readyState === EventSource.CLOSED) {
      setConnectionStatus(owner, "degraded");
      return;
    }
    setConnectionStatus(owner, "retrying");
  };

  if (eventTypes.length === 0) {
    source.onmessage = () => scheduleRefresh(owner);
  } else {
    eventTypes.forEach((type) => {
      source.addEventListener(type, () => scheduleRefresh(owner));
    });
  }

  setConnectionStatus(owner, "connecting");
};

export const initLiveEvents = (root = document) => {
  root.querySelectorAll?.("[data-live-events]").forEach(bindLiveEvents);
};
