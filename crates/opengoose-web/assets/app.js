import { initDashboardStreams } from "./modules/dashboard-stream.js";
import { initListShells } from "./modules/list-shell.js";
import { initLiveEvents } from "./modules/live-events.js";
import { initTableShells } from "./modules/table-shell.js";
import { initTheme } from "./modules/theme.js";
import { initWorkflowTriggers } from "./modules/workflow-trigger.js";

initTheme(document);
initListShells(document);
initTableShells(document);
initDashboardStreams(document);
initLiveEvents(document);
initWorkflowTriggers(document);
