const JSON_HEADERS = {
  accept: "application/json",
  "content-type": "application/json",
};

const metricLabel = (metric) => {
  switch (metric) {
    case "queue_backlog":
      return "Queue backlog";
    case "failed_runs":
      return "Failed runs";
    case "error_rate":
      return "Error runs";
    default:
      return String(metric || "");
  }
};

const formatValue = (value) => {
  const numeric = Number(value);
  if (Number.isNaN(numeric)) return String(value ?? "");
  return Number.isInteger(numeric) ? `${numeric}` : numeric.toFixed(2);
};

const readPayload = async (response) => response.json().catch(() => ({}));

const readErrorMessage = (response, payload) =>
  payload?.error || `${response.status} ${response.statusText}`.trim();

const setStatus = (element, message) => {
  if (element) {
    element.textContent = message;
  }
};

const buildHistoryRow = (entry) => {
  const row = document.createElement("tr");
  row.setAttribute("data-table-row", "");

  const ruleCell = document.createElement("td");
  const ruleLink = document.createElement("a");
  ruleLink.href = `/alerts?alert=${encodeURIComponent(entry.rule_name || "")}`;
  ruleLink.textContent = entry.rule_name || "Unknown rule";
  ruleCell.append(ruleLink);

  const metricCell = document.createElement("td");
  metricCell.textContent = metricLabel(entry.metric);

  const valueCell = document.createElement("td");
  valueCell.textContent = formatValue(entry.value);

  const triggeredAtCell = document.createElement("td");
  triggeredAtCell.textContent = entry.triggered_at || "";

  row.append(ruleCell, metricCell, valueCell, triggeredAtCell);
  return row;
};

const renderTestResult = (root, payload) => {
  const container = root.querySelector("[data-alert-test-result]");
  if (!container) return;

  const triggered = Array.isArray(payload?.triggered) ? payload.triggered : [];
  const metrics = payload?.metrics || {};

  const headline = document.createElement("strong");
  headline.textContent = triggered.length
    ? `${triggered.length} rule(s) triggered in the latest snapshot.`
    : "Snapshot complete. No rules triggered.";

  const snapshot = document.createElement("p");
  snapshot.textContent = `Queue backlog ${formatValue(
    metrics.queue_backlog
  )} · Failed runs ${formatValue(metrics.failed_runs)} · Error runs ${formatValue(
    metrics.error_rate
  )}`;

  container.hidden = false;
  if (triggered.length) {
    const detail = document.createElement("p");
    detail.textContent = `Triggered rules: ${triggered.join(", ")}`;
    container.replaceChildren(headline, snapshot, detail);
  } else {
    container.replaceChildren(headline, snapshot);
  }
};

const refreshHistory = async (root) => {
  const url = root.dataset.alertHistoryUrl;
  const body = root.querySelector("[data-alert-history-body]");
  const table = root.querySelector("[data-alert-history-table]");
  const empty = root.querySelector("[data-alert-history-empty]");
  if (!url || !body || !table || !empty) return;

  const response = await fetch(url, { headers: { accept: "application/json" } });
  const payload = await readPayload(response);
  if (!response.ok) {
    throw new Error(readErrorMessage(response, payload));
  }

  const entries = Array.isArray(payload) ? payload : [];
  body.replaceChildren(...entries.map(buildHistoryRow));
  table.hidden = entries.length === 0;
  empty.hidden = entries.length !== 0;
};

const bindCreateForm = (root, form) => {
  if (form.dataset.alertCreateBound === "true") return;
  form.dataset.alertCreateBound = "true";

  const status = form.querySelector("[data-alert-create-status]");
  const submit = form.querySelector("button[type='submit']");

  form.addEventListener("submit", async (event) => {
    event.preventDefault();
    const data = new FormData(form);
    const name = String(data.get("name") || "").trim();
    const description = String(data.get("description") || "").trim();
    const threshold = Number.parseFloat(String(data.get("threshold") || ""));

    if (!Number.isFinite(threshold)) {
      setStatus(status, "Threshold must be a finite number.");
      return;
    }

    if (submit) submit.disabled = true;
    setStatus(status, "Creating alert rule…");

    try {
      const response = await fetch(form.dataset.createUrl || "/api/alerts", {
        method: "POST",
        headers: JSON_HEADERS,
        body: JSON.stringify({
          name,
          description: description || null,
          metric: String(data.get("metric") || ""),
          condition: String(data.get("condition") || ""),
          threshold,
        }),
      });
      const payload = await readPayload(response);
      if (!response.ok) {
        throw new Error(readErrorMessage(response, payload));
      }

      setStatus(status, `Alert ${payload.name || name} created. Redirecting…`);
      window.location.assign(`/alerts?alert=${encodeURIComponent(payload.name || name)}`);
    } catch (error) {
      setStatus(
        status,
        error instanceof Error ? error.message : "Alert creation failed."
      );
    } finally {
      if (submit) submit.disabled = false;
    }
  });
};

const bindDeleteForm = (form) => {
  if (form.dataset.alertDeleteBound === "true") return;
  form.dataset.alertDeleteBound = "true";

  const status = form.querySelector("[data-alert-delete-status]");
  const confirm = form.querySelector("[data-alert-delete-confirm]");
  const submit = form.querySelector("button[type='submit']");

  form.addEventListener("submit", async (event) => {
    event.preventDefault();

    if (confirm && !confirm.checked) {
      setStatus(status, "Confirm deletion before removing this rule.");
      return;
    }

    if (submit) submit.disabled = true;
    setStatus(status, "Deleting alert rule…");

    try {
      const response = await fetch(form.dataset.deleteUrl || "", {
        method: "DELETE",
        headers: { accept: "application/json" },
      });
      const payload = await readPayload(response);
      if (!response.ok) {
        throw new Error(readErrorMessage(response, payload));
      }

      setStatus(status, "Alert deleted. Redirecting…");
      window.location.assign("/alerts");
    } catch (error) {
      setStatus(
        status,
        error instanceof Error ? error.message : "Alert deletion failed."
      );
    } finally {
      if (submit) submit.disabled = false;
    }
  });
};

const bindTestButtons = (root) => {
  const buttons = Array.from(root.querySelectorAll("[data-alert-run-test]"));
  if (!buttons.length) return;

  buttons.forEach((button) => {
    if (button.dataset.alertTestBound === "true") return;
    button.dataset.alertTestBound = "true";

    button.addEventListener("click", async () => {
      const status = root.querySelector("[data-alert-test-status]");
      buttons.forEach((candidate) => {
        candidate.disabled = true;
      });
      setStatus(status, "Running alert snapshot…");

      try {
        const response = await fetch(button.dataset.testUrl || "/api/alerts/test", {
          method: "POST",
          headers: { accept: "application/json" },
        });
        const payload = await readPayload(response);
        if (!response.ok) {
          throw new Error(readErrorMessage(response, payload));
        }

        const triggered = Array.isArray(payload.triggered) ? payload.triggered : [];
        setStatus(
          status,
          triggered.length
            ? `Triggered: ${triggered.join(", ")}`
            : "No enabled rules triggered."
        );
        renderTestResult(root, payload);
        try {
          await refreshHistory(root);
        } catch (refreshError) {
          setStatus(
            status,
            refreshError instanceof Error
              ? `Snapshot recorded, but history refresh failed: ${refreshError.message}`
              : "Snapshot recorded, but history refresh failed."
          );
        }
      } catch (error) {
        setStatus(
          status,
          error instanceof Error ? error.message : "Alert test failed."
        );
      } finally {
        buttons.forEach((candidate) => {
          candidate.disabled = false;
        });
      }
    });
  });
};

export const initAlertsPage = (root = document) => {
  root.querySelectorAll?.("[data-alerts-page]").forEach((page) => {
    page
      .querySelectorAll("[data-alert-create]")
      .forEach((form) => bindCreateForm(page, form));
    page.querySelectorAll("[data-alert-delete]").forEach(bindDeleteForm);
    bindTestButtons(page);
  });
};
