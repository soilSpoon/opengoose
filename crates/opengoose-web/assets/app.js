import { initDashboardStreams } from "./modules/dashboard-stream.js";
import { initListShells } from "./modules/list-shell.js";
import { initTableShells } from "./modules/table-shell.js";
import { initTheme } from "./modules/theme.js";

initTheme(document);
initListShells(document);
initTableShells(document);
initDashboardStreams(document);
