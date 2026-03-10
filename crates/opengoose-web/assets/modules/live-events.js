import { initDashboardStreams } from "./dashboard-stream.js";
import { initListShells } from "./list-shell.js";
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

const REFRESH_DEBOUNCE_MS = 180;
const INITIAL_RETRY_DELAY_MS = 1000;
const MAX_RETRY_DELAY_MS = 30000;

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

const reconnectDelayMs = (attempt) =>
  Math.min(INITIAL_RETRY_DELAY_MS * 2 ** attempt, MAX_RETRY_DELAY_MS);

const closeSource = (state) => {
  if (!state?.source) return;

  state.source.onopen = null;
  state.source.onerror = null;
  state.source.close();
  state.source = null;
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
  initLiveEvents(document);
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
  }, REFRESH_DEBOUNCE_MS);
};

const bindEventTypes = (source, owner, eventTypes) => {
  if (eventTypes.length === 0) {
    source.onmessage = () => scheduleRefresh(owner);
  } else {
    eventTypes.forEach((type) => {
      source.addEventListener(type, () => scheduleRefresh(owner));
    });
  }
};

const scheduleReconnect = (owner) => {
  const state = liveEventState.get(owner);
  if (!state || state.reconnectTimer) return;

  const attempt = state.retryAttempts;
  state.retryAttempts += 1;
  setConnectionStatus(owner, "retrying");

  state.reconnectTimer = window.setTimeout(() => {
    state.reconnectTimer = null;
    connectLiveEvents(owner);
  }, reconnectDelayMs(attempt));
};

const connectLiveEvents = (owner) => {
  const state = liveEventState.get(owner);
  if (!state) return;

  closeSource(state);
  setConnectionStatus(
    owner,
    state.retryAttempts > 0 ? "retrying" : "connecting",
  );

  const source = new EventSource(state.url);
  state.source = source;

  source.onopen = () => {
    if (state.source !== source) return;

    state.retryAttempts = 0;
    setConnectionStatus(owner, "live");
  };

  source.onerror = () => {
    if (state.source !== source) return;

    closeSource(state);
    scheduleReconnect(owner);
  };

  bindEventTypes(source, owner, state.eventTypes);
};

const bindLiveEvents = (owner) => {
  if (!owner || owner.dataset.liveEventsBound === "true") return;

  const eventTypes = parseCsv(owner.dataset.liveEventsTypes);
  const selectors = parseCsv(owner.dataset.liveEventsTargets);
  if (selectors.length === 0) return;

  owner.dataset.liveEventsBound = "true";

  liveEventState.set(owner, {
    eventTypes,
    selectors,
    url:
      owner.dataset.liveEventsUrl ||
      `/api/events${eventTypes.length ? `?types=${eventTypes.join(",")}` : ""}`,
    pendingTimer: null,
    reconnectTimer: null,
    refreshing: false,
    retryAttempts: 0,
    source: null,
  });

  connectLiveEvents(owner);
};

export const initLiveEvents = (root = document) => {
  root.querySelectorAll?.("[data-live-events]").forEach(bindLiveEvents);
};
