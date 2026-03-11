const JSON_HEADERS = {
  accept: "application/json",
  "content-type": "application/json",
};

const normalize = (value) => (value || "").trim().toLowerCase();

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

const conditionLabel = (condition) => {
  switch (condition) {
    case "gt":
      return "Greater than";
    case "lt":
      return "Less than";
    case "gte":
      return "Greater than or equal";
    case "lte":
      return "Less than or equal";
    default:
      return String(condition || "");
  }
};

const conditionSymbol = (condition) => {
  switch (condition) {
    case "gt":
      return ">";
    case "lt":
      return "<";
    case "gte":
      return ">=";
    case "lte":
      return "<=";
    default:
      return "=?";
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

const parseBoolean = (value) => value === "true";
const coerceFiniteNumber = (value) => {
  const numeric = Number(value);
  return Number.isFinite(numeric) ? numeric : null;
};

const createMetricCard = ({ label, value, note, tone = "neutral" }) => {
  const card = document.createElement("article");
  card.className = `metric-card tone-${tone}`;

  const metricLabelEl = document.createElement("p");
  metricLabelEl.className = "metric-label";
  metricLabelEl.textContent = label;

  const metricValueEl = document.createElement("strong");
  metricValueEl.className = "metric-value";
  metricValueEl.textContent = value;

  const metricNoteEl = document.createElement("p");
  metricNoteEl.className = "metric-note";
  metricNoteEl.textContent = note;

  card.append(metricLabelEl, metricValueEl, metricNoteEl);
  return card;
};

const createChip = (label, tone = "neutral") => {
  const chip = document.createElement("span");
  chip.className = `chip tone-${tone}`;
  chip.textContent = label;
  return chip;
};

const buildTargetLabel = (rule) =>
  rule?.targetLabel ||
  `${rule?.metricLabel || metricLabel(rule?.metricKey)} ${conditionSymbol(
    rule?.conditionKey
  )} ${formatValue(rule?.thresholdValue)}`.trim();

const getRules = (root) =>
  Array.from(root.querySelectorAll("[data-alert-rule]")).map((node) => ({
    name: node.dataset.ruleName || "",
    enabled: parseBoolean(node.dataset.ruleEnabled),
    metricKey: node.dataset.ruleMetric || "",
    metricLabel: node.dataset.ruleMetricLabel || metricLabel(node.dataset.ruleMetric),
    conditionKey: node.dataset.ruleCondition || "",
    conditionLabel:
      node.dataset.ruleConditionLabel || conditionLabel(node.dataset.ruleCondition),
    thresholdValue: Number(node.dataset.ruleThreshold),
    thresholdLabel: node.dataset.ruleThresholdLabel || formatValue(node.dataset.ruleThreshold),
    targetLabel: node.dataset.ruleTarget || "",
    active: node.classList.contains("is-active"),
  }));

const getRuleIndex = (root) =>
  new Map(getRules(root).map((rule) => [normalize(rule.name), rule]));

const getActiveRule = (root) => getRules(root).find((rule) => rule.active) || null;

const readMetricValue = (metrics, metricKey) => {
  const numeric = Number(metrics?.[metricKey]);
  return Number.isFinite(numeric) ? numeric : null;
};

const evaluateCondition = (condition, observed, threshold) => {
  if (!Number.isFinite(observed) || !Number.isFinite(threshold)) return false;

  switch (condition) {
    case "gt":
      return observed > threshold;
    case "lt":
      return observed < threshold;
    case "gte":
      return observed >= threshold;
    case "lte":
      return observed <= threshold;
    default:
      return false;
  }
};

const metricCards = (metrics) => [
  {
    label: "Queue backlog",
    value: formatValue(metrics.queue_backlog),
    note: "Pending or failed queue entries currently waiting on recovery.",
    tone: "amber",
  },
  {
    label: "Failed runs",
    value: formatValue(metrics.failed_runs),
    note: "Persisted orchestration runs marked as failed.",
    tone: "rose",
  },
  {
    label: "Error runs",
    value: formatValue(metrics.error_rate),
    note: "Persisted orchestration runs marked as error.",
    tone: "cyan",
  },
];

const normalizeTriggered = (payload) => {
  const raw =
    payload?.triggered ||
    payload?.triggered_rules ||
    payload?.triggeredRules ||
    payload?.matches ||
    [];

  if (!Array.isArray(raw)) return [];

  return raw
    .map((entry) => {
      if (typeof entry === "string") return entry;
      return entry?.rule_name || entry?.name || entry?.rule || "";
    })
    .filter(Boolean);
};

const normalizeMetrics = (payload) =>
  payload?.metrics || payload?.snapshot?.metrics || payload?.observed_metrics || {};

const normalizeContexts = (payload) => {
  const raw =
    payload?.context ||
    payload?.contexts ||
    payload?.notes ||
    payload?.details ||
    payload?.messages ||
    [];

  if (Array.isArray(raw)) {
    return raw
      .map((item) => {
        if (typeof item === "string") return item;
        return item?.message || item?.detail || item?.reason || "";
      })
      .filter(Boolean);
  }

  if (raw && typeof raw === "object") {
    return Object.entries(raw)
      .map(([key, value]) => `${key}: ${value}`)
      .filter(Boolean);
  }

  return typeof raw === "string" && raw ? [raw] : [];
};

const resultTone = (matched, label) => {
  if (matched) return "rose";
  if (normalize(label).includes("skip")) return "neutral";
  return "success";
};

const normalizeEvaluations = (payload, root) => {
  const rules = getRuleIndex(root);
  const metrics = normalizeMetrics(payload);
  const triggeredNames = normalizeTriggered(payload);
  const triggeredSet = new Set(triggeredNames.map(normalize));
  const raw =
    payload?.evaluations || payload?.results || payload?.rules || payload?.items || null;

  if (Array.isArray(raw) && raw.length) {
    return raw.map((entry, index) => {
      const name =
        entry?.rule_name || entry?.name || entry?.rule || `Rule ${index + 1}`;
      const fallbackRule = rules.get(normalize(name));
      const metricKey = entry?.metric || fallbackRule?.metricKey || "";
      const observed =
        coerceFiniteNumber(entry?.observed_value ?? entry?.observed ?? entry?.value) ??
        readMetricValue(metrics, metricKey);
      const threshold =
        coerceFiniteNumber(entry?.threshold ?? entry?.target_value) ??
        fallbackRule?.thresholdValue;
      const matched =
        typeof entry?.triggered === "boolean"
          ? entry.triggered
          : typeof entry?.matched === "boolean"
            ? entry.matched
            : triggeredSet.has(normalize(name)) ||
              evaluateCondition(
                entry?.condition || fallbackRule?.conditionKey,
                observed,
                threshold
              );
      const resultLabel =
        entry?.result ||
        entry?.status ||
        (matched ? "Triggered" : "Within target");

      return {
        name,
        metricLabel:
          entry?.metric_label || fallbackRule?.metricLabel || metricLabel(metricKey),
        observedLabel: formatValue(observed),
        targetLabel:
          entry?.target ||
          entry?.target_label ||
          (fallbackRule
            ? buildTargetLabel(fallbackRule)
            : `${metricLabel(metricKey)} ${conditionSymbol(
                entry?.condition
              )} ${formatValue(threshold)}`.trim()),
        resultLabel,
        tone: resultTone(matched, resultLabel),
        context:
          entry?.context || entry?.reason || entry?.detail || entry?.message || "",
        focus: fallbackRule?.active || false,
      };
    });
  }

  return Array.from(rules.values())
    .filter((rule) => rule.enabled)
    .map((rule) => {
      const observed = readMetricValue(metrics, rule.metricKey);
      const matched =
        triggeredSet.size > 0
          ? triggeredSet.has(normalize(rule.name))
          : evaluateCondition(rule.conditionKey, observed, rule.thresholdValue);

      return {
        name: rule.name,
        metricLabel: rule.metricLabel,
        observedLabel: formatValue(observed),
        targetLabel: buildTargetLabel(rule),
        resultLabel: matched ? "Triggered" : "Within target",
        tone: matched ? "rose" : "success",
        context: matched
          ? `${rule.metricLabel} crossed ${rule.conditionLabel.toLowerCase()} ${rule.thresholdLabel}.`
          : `${rule.metricLabel} remains within target.`,
        focus: rule.active,
      };
    });
};

const buildHistoryFilters = (root) => {
  const scope = root.querySelector("[data-alert-history-scope]");
  const since = root.querySelector("[data-alert-history-since]");
  const limit = root.querySelector("[data-alert-history-limit]");
  const activeRule = getActiveRule(root);

  return {
    scope: scope?.value || "selected",
    sinceRaw: since?.value || "",
    sinceParam:
      since?.value && !Number.isNaN(new Date(since.value).getTime())
        ? new Date(since.value).toISOString()
        : since?.value || "",
    limit: Number.parseInt(limit?.value || "25", 10) || 25,
    activeRule,
  };
};

const buildHistoryUrl = (root, filters) => {
  const base = root.dataset.alertHistoryUrl || "/api/alerts/history";
  const url = new URL(base, window.location.origin);

  if (filters.scope === "selected" && filters.activeRule?.name) {
    url.searchParams.set("rule", filters.activeRule.name);
  }
  if (filters.sinceParam) {
    url.searchParams.set("since", filters.sinceParam);
  }
  url.searchParams.set("limit", `${filters.limit}`);
  return url.toString();
};

const historyEntriesFromPayload = (payload) => {
  if (Array.isArray(payload)) return payload;
  if (Array.isArray(payload?.items)) return payload.items;
  if (Array.isArray(payload?.data)) return payload.data;
  return [];
};

const normalizeHistoryEntry = (entry, rules) => {
  const name = entry?.rule_name || entry?.name || entry?.rule || "Unknown rule";
  const fallbackRule = rules.get(normalize(name));
  const metricKey = entry?.metric || fallbackRule?.metricKey || "";
  const resultLabel =
    entry?.result ||
    entry?.status ||
    (entry?.triggered === false ? "Within target" : "Triggered");

  return {
    name,
    metricLabel:
      entry?.metric_label || fallbackRule?.metricLabel || metricLabel(metricKey),
    valueLabel: formatValue(entry?.value),
    resultLabel,
    resultTone: resultTone(normalize(resultLabel) === "triggered", resultLabel),
    targetLabel:
      entry?.target ||
      entry?.target_label ||
      (fallbackRule ? buildTargetLabel(fallbackRule) : "Rule definition unavailable"),
    triggeredAt: entry?.triggered_at || entry?.timestamp || entry?.created_at || "",
    context: entry?.context || entry?.reason || entry?.detail || "",
  };
};

const filterHistoryEntries = (entries, filters) => {
  const sinceTime = filters.sinceRaw ? new Date(filters.sinceRaw).getTime() : null;

  return entries
    .filter((entry) => {
      if (filters.scope === "selected" && filters.activeRule?.name) {
        return normalize(entry.name) === normalize(filters.activeRule.name);
      }
      return true;
    })
    .filter((entry) => {
      if (!Number.isFinite(sinceTime)) return true;
      const entryTime = new Date(entry.triggeredAt).getTime();
      return Number.isFinite(entryTime) ? entryTime >= sinceTime : true;
    })
    .slice(0, filters.limit);
};

const buildHistoryRow = (entry) => {
  const row = document.createElement("tr");
  row.setAttribute("data-table-row", "");

  const ruleCell = document.createElement("td");
  const ruleLink = document.createElement("a");
  ruleLink.href = `/alerts?alert=${encodeURIComponent(entry.name)}`;
  ruleLink.textContent = entry.name;
  ruleCell.append(ruleLink);

  const metricCell = document.createElement("td");
  metricCell.textContent = entry.metricLabel;

  const valueCell = document.createElement("td");
  valueCell.textContent = entry.valueLabel;

  const resultCell = document.createElement("td");
  resultCell.append(createChip(entry.resultLabel, entry.resultTone));
  if (entry.context) {
    const detail = document.createElement("p");
    detail.className = "table-detail-copy";
    detail.textContent = entry.context;
    resultCell.append(detail);
  }

  const targetCell = document.createElement("td");
  targetCell.textContent = entry.targetLabel;

  const triggeredAtCell = document.createElement("td");
  triggeredAtCell.textContent = entry.triggeredAt;

  row.append(ruleCell, metricCell, valueCell, resultCell, targetCell, triggeredAtCell);
  return row;
};

const renderHistory = (root, entries, filters) => {
  const body = root.querySelector("[data-alert-history-body]");
  const table = root.querySelector("[data-alert-history-table]");
  const empty = root.querySelector("[data-alert-history-empty]");
  const status = root.querySelector("[data-alert-history-status]");

  if (!body || !table || !empty) return;

  body.replaceChildren(...entries.map(buildHistoryRow));
  table.hidden = entries.length === 0;
  empty.hidden = entries.length !== 0;

  if (entries.length === 0) {
    empty.textContent = filters.scope === "selected"
      ? "No trigger history matched the selected alert and time window."
      : "No trigger history matched the current filters.";
  }

  if (status) {
    const scopeLabel =
      filters.scope === "selected" && filters.activeRule?.name
        ? filters.activeRule.name
        : "all alerts";
    status.textContent = entries.length
      ? `Showing ${entries.length} event(s) for ${scopeLabel}.`
      : `No history matched for ${scopeLabel}.`;
  }
};

const refreshHistory = async (root) => {
  const rules = getRuleIndex(root);
  const filters = buildHistoryFilters(root);
  const response = await fetch(buildHistoryUrl(root, filters), {
    headers: { accept: "application/json" },
  });
  const payload = await readPayload(response);

  if (!response.ok) {
    throw new Error(readErrorMessage(response, payload));
  }

  const entries = filterHistoryEntries(
    historyEntriesFromPayload(payload).map((entry) => normalizeHistoryEntry(entry, rules)),
    filters
  );
  renderHistory(root, entries, filters);
};

const buildEvaluationRow = (evaluation) => {
  const row = document.createElement("tr");
  row.setAttribute("data-table-row", "");
  if (evaluation.focus) row.classList.add("alert-evaluation-row-focus");

  const ruleCell = document.createElement("td");
  const name = document.createElement("strong");
  name.textContent = evaluation.name;
  ruleCell.append(name);
  if (evaluation.focus) {
    ruleCell.append(document.createTextNode(" "));
    ruleCell.append(createChip("Selected", "cyan"));
  }

  const metricCell = document.createElement("td");
  metricCell.textContent = evaluation.metricLabel;

  const observedCell = document.createElement("td");
  observedCell.textContent = evaluation.observedLabel;

  const targetCell = document.createElement("td");
  targetCell.textContent = evaluation.targetLabel;

  const resultCell = document.createElement("td");
  resultCell.append(createChip(evaluation.resultLabel, evaluation.tone));
  if (evaluation.context) {
    const detail = document.createElement("p");
    detail.className = "table-detail-copy";
    detail.textContent = evaluation.context;
    resultCell.append(detail);
  }

  row.append(ruleCell, metricCell, observedCell, targetCell, resultCell);
  return row;
};

const renderSnapshot = (root, payload) => {
  const status = root.querySelector("[data-alert-snapshot-status]");
  const empty = root.querySelector("[data-alert-snapshot-empty]");
  const snapshot = root.querySelector("[data-alert-snapshot]");
  const metricsContainer = root.querySelector("[data-alert-snapshot-metrics]");
  const triggeredContainer = root.querySelector("[data-alert-triggered-rules]");
  const triggeredEmpty = root.querySelector("[data-alert-triggered-empty]");
  const evaluationsBody = root.querySelector("[data-alert-evaluations-body]");
  const contextContainer = root.querySelector("[data-alert-context-list]");

  if (!snapshot || !metricsContainer || !triggeredContainer || !evaluationsBody) return;

  const metrics = normalizeMetrics(payload);
  const triggeredNames = normalizeTriggered(payload);
  const evaluations = normalizeEvaluations(payload, root);
  const contexts = normalizeContexts(payload);

  metricsContainer.replaceChildren(...metricCards(metrics).map(createMetricCard));
  triggeredContainer.replaceChildren(
    ...triggeredNames.map((name) => createChip(name, "rose"))
  );
  if (triggeredEmpty) triggeredEmpty.hidden = triggeredNames.length !== 0;

  evaluationsBody.replaceChildren(...evaluations.map(buildEvaluationRow));
  snapshot.hidden = false;
  if (empty) empty.hidden = true;

  if (contextContainer) {
    if (contexts.length) {
      contextContainer.hidden = false;
      contextContainer.replaceChildren(
        ...contexts.map((message) => {
          const line = document.createElement("p");
          line.textContent = message;
          return line;
        })
      );
    } else {
      contextContainer.hidden = true;
      contextContainer.replaceChildren();
    }
  }

  if (status) {
    status.textContent = triggeredNames.length
      ? `${triggeredNames.length} rule(s) triggered in the latest snapshot.`
      : "Snapshot complete. No enabled rules triggered.";
  }
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

const bindHistoryFilters = (root) => {
  const form = root.querySelector("[data-alert-history-filters]");
  if (!form || form.dataset.alertHistoryBound === "true") return;
  form.dataset.alertHistoryBound = "true";

  const status = form.querySelector("[data-alert-history-status]");
  const reset = form.querySelector("[data-alert-history-reset]");
  const scope = form.querySelector("[data-alert-history-scope]");
  const since = form.querySelector("[data-alert-history-since]");
  const limit = form.querySelector("[data-alert-history-limit]");

  const refresh = async () => {
    setStatus(status, "Refreshing history…");

    try {
      await refreshHistory(root);
    } catch (error) {
      setStatus(
        status,
        error instanceof Error ? error.message : "History refresh failed."
      );
    }
  };

  form.addEventListener("submit", async (event) => {
    event.preventDefault();
    await refresh();
  });

  reset?.addEventListener("click", async () => {
    if (scope) scope.value = getActiveRule(root) ? "selected" : "all";
    if (since) since.value = "";
    if (limit) limit.value = "25";
    await refresh();
  });

  refresh().catch(() => {});
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

        const triggered = normalizeTriggered(payload);
        setStatus(
          status,
          triggered.length
            ? `Triggered: ${triggered.join(", ")}`
            : "No enabled rules triggered."
        );
        renderSnapshot(root, payload);
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
    bindHistoryFilters(page);
    bindTestButtons(page);
  });
};
