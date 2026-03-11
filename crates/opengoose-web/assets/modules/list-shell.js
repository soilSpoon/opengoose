const normalize = (value) => (value || "").trim().toLowerCase();

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

export const initListShells = (root = document) => {
  root.querySelectorAll?.("[data-list-shell]").forEach(enhanceListShell);
};
