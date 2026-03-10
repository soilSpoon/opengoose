# OpenGoose Web Dashboard Guide

## Launch

Start the dashboard from the repo root:

```bash
opengoose web --port 8080
```

Open `http://127.0.0.1:8080`.

## What the dashboard covers

The dashboard stays server-rendered and adds live updates only where they help operators:

- `Dashboard`: live SSE snapshot of recent sessions, runs, queue pressure, and agent activity.
- `Sessions`: conversation history with a searchable session rail and HTMX detail panel.
- `Runs`: orchestration status, work items, and broadcasts for a selected run.
- `Agents`: installed agent profiles, extensions, skills, and YAML.
- `Teams`: editable team definitions with inline validation and save feedback.
- `Queue`: searchable queue traffic with client-side filtering, sorting, and pagination.

## Interaction model

### Search and paging

- The left rail on `Sessions`, `Runs`, `Agents`, `Teams`, and `Queue` includes search and page-size controls.
- Filters apply client-side so operators can narrow the current page without a full refresh.
- Pager controls keep the rail compact even when the catalog grows.

### Keyboard support

- Use `Tab` to move between controls, navigation, and the detail panel.
- On a focused rail item, use `ArrowUp`, `ArrowDown`, `Home`, and `End` to move through visible entries.
- On a focused queue row, use the same keys to move through visible message rows.
- A skip link jumps directly to the main content region.

### Loading and error feedback

- HTMX detail loads mark the panel as busy and show an inline loading state.
- Failed detail requests surface an inline alert instead of silently failing.
- Dashboard SSE connection state is announced directly in the hero status area.

## Screenshots

### Dashboard overview

![Dashboard overview](images/web-dashboard/dashboard-overview.png)

### Sessions rail and detail panel

![Sessions rail and detail panel](images/web-dashboard/sessions-detail.png)

### Queue controls and sortable table

![Queue controls and table](images/web-dashboard/queue-table.png)
