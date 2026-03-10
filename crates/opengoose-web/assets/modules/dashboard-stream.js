const connectionTones = {
  connecting: "amber",
  live: "success",
  retrying: "amber",
  degraded: "rose",
  paused: "neutral",
};

const setConnectionStatus = (root, state) => {
  const connection = root?.querySelector("[data-dashboard-connection]");
  if (!connection) return;

  const label = {
    connecting: "Connecting",
    live: "SSE live",
    retrying: "Reconnecting",
    degraded: "Stream degraded",
    paused: "Stream paused",
  }[state];

  connection.textContent = label;
  connection.className = `chip tone-${connectionTones[state]}`;
};

let fetchLifecycleBound = false;

const onFetchLifecycle = (event) => {
  const stream = event.target?.closest?.("[data-dashboard-stream]");
  const owner = stream?.closest?.("[data-dashboard-stream-root]");
  if (!stream || !owner) return;

  switch (event.detail?.type) {
    case "started":
      setConnectionStatus(owner, "live");
      break;
    case "retrying":
      setConnectionStatus(owner, "retrying");
      break;
    case "error":
    case "retries-failed":
      setConnectionStatus(owner, "degraded");
      break;
    case "finished":
      setConnectionStatus(owner, "paused");
      break;
    default:
      break;
  }
};

export const initDashboardStreams = (root = document) => {
  root.querySelectorAll?.("[data-dashboard-stream]").forEach((stream) => {
    const owner = stream.closest("[data-dashboard-stream-root]");
    if (!owner) return;
    setConnectionStatus(owner, "connecting");
  });

  if (fetchLifecycleBound) return;
  fetchLifecycleBound = true;
  document.addEventListener("datastar-fetch", onFetchLifecycle);
};
