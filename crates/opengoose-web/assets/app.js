(() => {
  const collator = new Intl.Collator(undefined, {
    numeric: true,
    sensitivity: "base",
  });

  const normalize = (value) => (value || "").trim().toLowerCase();
  const parseNumber = (value) => {
    const parsed = Number.parseInt(`${value || ""}`.replace(/[^\d-]/g, ""), 10);
    return Number.isNaN(parsed) ? 0 : parsed;
  };

  const syncThemeButton = () => {
    const button = document.querySelector(".theme-toggle");
    if (!button) return;

    const isDark = document.documentElement.dataset.theme === "dark";
    button.setAttribute("aria-pressed", isDark ? "true" : "false");
    button.textContent = isDark ? "Theme: Dark" : "Theme: Light";
  };

  const markActiveRailItem = (item) => {
    const shell = item?.closest("[data-list-shell]");
    if (!shell) return;

    shell.querySelectorAll("[data-list-item]").forEach((candidate) => {
      const isActive = candidate === item;
      candidate.classList.toggle("is-active", isActive);
      if (isActive) {
        candidate.setAttribute("aria-current", "page");
      } else {
        candidate.removeAttribute("aria-current");
      }
    });
  };

  const enhanceListShell = (shell) => {
    if (!shell || shell.dataset.enhanced === "true") return;
    shell.dataset.enhanced = "true";

    const label = shell.dataset.listLabel || "items";
    const search = shell.querySelector("[data-list-search]");
    const pageSize = shell.querySelector("[data-list-page-size]");
    const status = shell.querySelector("[data-list-status]");
    const empty = shell.querySelector("[data-list-empty]");
    const prev = shell.querySelector("[data-list-prev]");
    const next = shell.querySelector("[data-list-next]");
    const page = shell.querySelector("[data-list-page]");
    const items = Array.from(shell.querySelectorAll("[data-list-item]"));

    let currentPage = 0;

    const render = () => {
      const query = normalize(search?.value);
      const visibleItems = items.filter((item) =>
        normalize(item.dataset.search).includes(query)
      );
      const size = Number.parseInt(pageSize?.value || "8", 10) || 8;
      const pageCount = Math.max(1, Math.ceil(visibleItems.length / size));
      currentPage = Math.min(currentPage, pageCount - 1);

      const start = currentPage * size;
      const pageItems = new Set(visibleItems.slice(start, start + size));

      items.forEach((item) => {
        const visible = pageItems.has(item);
        item.hidden = !visible;
        item.setAttribute("aria-hidden", visible ? "false" : "true");
        item.tabIndex = visible ? 0 : -1;
      });

      if (status) {
        status.textContent = visibleItems.length
          ? `Showing ${Math.min(size, visibleItems.length - start)} of ${visibleItems.length} ${label} · page ${currentPage + 1} of ${pageCount}`
          : `No ${label} match the current filter.`;
      }

      if (page) {
        page.textContent = visibleItems.length
          ? `Page ${currentPage + 1} of ${pageCount}`
          : "Page 0 of 0";
      }

      if (prev) prev.disabled = currentPage === 0 || visibleItems.length === 0;
      if (next) {
        next.disabled =
          visibleItems.length === 0 || currentPage >= pageCount - 1;
      }
      if (empty) empty.hidden = visibleItems.length !== 0;
    };

    search?.addEventListener("input", () => {
      currentPage = 0;
      render();
    });
    pageSize?.addEventListener("change", () => {
      currentPage = 0;
      render();
    });
    prev?.addEventListener("click", () => {
      currentPage = Math.max(0, currentPage - 1);
      render();
    });
    next?.addEventListener("click", () => {
      currentPage += 1;
      render();
    });

    shell.addEventListener("click", (event) => {
      const item = event.target.closest("[data-list-item]");
      if (item) markActiveRailItem(item);
    });

    shell.addEventListener("keydown", (event) => {
      const current = event.target.closest("[data-list-item]");
      if (!current) return;

      const visibleItems = items.filter((item) => !item.hidden);
      const index = visibleItems.indexOf(current);
      if (index === -1) return;

      let target = null;
      if (event.key === "ArrowDown") {
        target = visibleItems[Math.min(index + 1, visibleItems.length - 1)];
      } else if (event.key === "ArrowUp") {
        target = visibleItems[Math.max(index - 1, 0)];
      } else if (event.key === "Home") {
        target = visibleItems[0];
      } else if (event.key === "End") {
        target = visibleItems[visibleItems.length - 1];
      }

      if (!target || target === current) return;
      event.preventDefault();
      target.focus();
    });

    render();
  };

  const enhanceTableShell = (shell) => {
    if (!shell || shell.dataset.enhanced === "true") return;
    shell.dataset.enhanced = "true";

    const label = shell.dataset.tableLabel || "rows";
    const search = shell.querySelector("[data-table-search]");
    const statusFilter = shell.querySelector("[data-table-filter]");
    const sort = shell.querySelector("[data-table-sort]");
    const pageSize = shell.querySelector("[data-table-page-size]");
    const status = shell.querySelector("[data-table-status]");
    const empty = shell.querySelector("[data-table-empty]");
    const prev = shell.querySelector("[data-table-prev]");
    const next = shell.querySelector("[data-table-next]");
    const page = shell.querySelector("[data-table-page]");
    const body = shell.querySelector("[data-table-body]");

    if (!body) return;

    let currentPage = 0;

    const groups = () =>
      Array.from(body.querySelectorAll("[data-table-row]")).map((primary) => ({
        primary,
        detail:
          primary.nextElementSibling?.matches("[data-table-detail]")
            ? primary.nextElementSibling
            : null,
      }));

    const compareGroups = (left, right, mode) => {
      if (mode === "sender") {
        return collator.compare(
          left.primary.dataset.sortSender || "",
          right.primary.dataset.sortSender || ""
        );
      }
      if (mode === "recipient") {
        return collator.compare(
          left.primary.dataset.sortRecipient || "",
          right.primary.dataset.sortRecipient || ""
        );
      }
      if (mode === "retries") {
        return (
          parseNumber(right.primary.dataset.sortRetries) -
          parseNumber(left.primary.dataset.sortRetries)
        );
      }
      const leftCreated = left.primary.dataset.sortCreated || "";
      const rightCreated = right.primary.dataset.sortCreated || "";
      const order = collator.compare(leftCreated, rightCreated);
      return mode === "oldest" ? order : -order;
    };

    const render = () => {
      const query = normalize(search?.value);
      const selectedStatus = normalize(statusFilter?.value);
      const sorted = groups()
        .filter(({ primary }) => {
          const matchesSearch = normalize(primary.dataset.search).includes(query);
          const matchesStatus =
            !selectedStatus ||
            selectedStatus === "all" ||
            normalize(primary.dataset.status) === selectedStatus;
          return matchesSearch && matchesStatus;
        })
        .sort((left, right) => compareGroups(left, right, sort?.value || "newest"));

      sorted.forEach(({ primary, detail }) => {
        body.append(primary);
        if (detail) body.append(detail);
      });

      const size = Number.parseInt(pageSize?.value || "6", 10) || 6;
      const pageCount = Math.max(1, Math.ceil(sorted.length / size));
      currentPage = Math.min(currentPage, pageCount - 1);

      const start = currentPage * size;
      const pageRows = new Set(sorted.slice(start, start + size).map(({ primary }) => primary));

      groups().forEach(({ primary, detail }) => {
        const visible = pageRows.has(primary);
        primary.hidden = !visible;
        primary.tabIndex = visible ? 0 : -1;
        primary.setAttribute("aria-hidden", visible ? "false" : "true");
        if (detail) detail.hidden = !visible;
      });

      if (status) {
        status.textContent = sorted.length
          ? `Showing ${Math.min(size, sorted.length - start)} of ${sorted.length} ${label} · page ${currentPage + 1} of ${pageCount}`
          : `No ${label} match the current filters.`;
      }
      if (page) {
        page.textContent = sorted.length
          ? `Page ${currentPage + 1} of ${pageCount}`
          : "Page 0 of 0";
      }
      if (prev) prev.disabled = currentPage === 0 || sorted.length === 0;
      if (next) {
        next.disabled = sorted.length === 0 || currentPage >= pageCount - 1;
      }
      if (empty) empty.hidden = sorted.length !== 0;
    };

    search?.addEventListener("input", () => {
      currentPage = 0;
      render();
    });
    statusFilter?.addEventListener("change", () => {
      currentPage = 0;
      render();
    });
    sort?.addEventListener("change", () => {
      currentPage = 0;
      render();
    });
    pageSize?.addEventListener("change", () => {
      currentPage = 0;
      render();
    });
    prev?.addEventListener("click", () => {
      currentPage = Math.max(0, currentPage - 1);
      render();
    });
    next?.addEventListener("click", () => {
      currentPage += 1;
      render();
    });

    shell.addEventListener("keydown", (event) => {
      const current = event.target.closest("[data-table-row]");
      if (!current) return;

      const visibleRows = groups()
        .map(({ primary }) => primary)
        .filter((row) => !row.hidden);
      const index = visibleRows.indexOf(current);
      if (index === -1) return;

      let target = null;
      if (event.key === "ArrowDown") {
        target = visibleRows[Math.min(index + 1, visibleRows.length - 1)];
      } else if (event.key === "ArrowUp") {
        target = visibleRows[Math.max(index - 1, 0)];
      } else if (event.key === "Home") {
        target = visibleRows[0];
      } else if (event.key === "End") {
        target = visibleRows[visibleRows.length - 1];
      }

      if (!target || target === current) return;
      event.preventDefault();
      target.focus();
    });

    render();
  };

  const initialize = (root = document) => {
    root.querySelectorAll?.("[data-list-shell]").forEach(enhanceListShell);
    root.querySelectorAll?.("[data-table-shell]").forEach(enhanceTableShell);
  };

  const setPanelBusy = (panel, busy) => {
    if (!panel) return;
    panel.classList.toggle("is-loading", busy);
    panel.setAttribute("aria-busy", busy ? "true" : "false");
  };

  const findDetailContext = (target) => {
    const panel = target?.closest?.("[data-detail-panel]");
    const shell = target?.closest?.("[data-detail-shell]");
    if (!shell) return null;

    return {
      shell,
      panel: panel || shell.querySelector("[data-detail-panel]"),
      status: shell.querySelector("[data-detail-status]"),
      feedback: shell.querySelector("[data-shell-feedback]"),
    };
  };

  document.addEventListener("click", (event) => {
    if (event.target.closest(".theme-toggle")) {
      window.setTimeout(syncThemeButton, 0);
    }
  });

  document.body.addEventListener("htmx:beforeRequest", (event) => {
    const context = findDetailContext(event.detail.target);
    if (!context) return;

    setPanelBusy(context.panel, true);
    if (context.status) context.status.textContent = "Loading panel content.";
    if (context.feedback) {
      context.feedback.hidden = true;
      context.feedback.textContent = "";
    }
  });

  document.body.addEventListener("htmx:afterSwap", (event) => {
    const context = findDetailContext(event.detail.target);
    if (!context) return;

    setPanelBusy(context.panel, false);
    if (context.status) context.status.textContent = "Panel content updated.";
    initialize(context.panel);
    context.panel?.focus?.({ preventScroll: true });
  });

  const onDetailError = (event) => {
    const context = findDetailContext(event.detail.target);
    if (!context) return;

    setPanelBusy(context.panel, false);
    if (context.status) context.status.textContent = "Panel content failed to load.";
    if (context.feedback) {
      context.feedback.hidden = false;
      context.feedback.textContent =
        "The dashboard could not load the requested panel. Retry the action or refresh the page.";
    }
  };

  document.body.addEventListener("htmx:responseError", onDetailError);
  document.body.addEventListener("htmx:sendError", onDetailError);

  syncThemeButton();
  initialize(document);
})();
