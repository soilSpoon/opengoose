const collator = new Intl.Collator(undefined, {
  numeric: true,
  sensitivity: "base",
});

const normalize = (value) => (value || "").trim().toLowerCase();
const parseNumber = (value) => {
  const parsed = Number.parseInt(`${value || ""}`.replace(/[^\d-]/g, ""), 10);
  return Number.isNaN(parsed) ? 0 : parsed;
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
    const pageRows = new Set(
      sorted.slice(start, start + size).map(({ primary }) => primary)
    );

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

export const initTableShells = (root = document) => {
  root.querySelectorAll?.("[data-table-shell]").forEach(enhanceTableShell);
};
