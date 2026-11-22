/** The routes of the app's area: fleet and the places that show what the machines are doing: the Inbox, Fleet, a run, the Terminal and the three tabs of Fleet. Pages are fetched when their route opens. */
import { lazy } from "react";

const InboxPage = lazy(() =>
  import("../inbox/InboxPage").then((all) => ({ default: all.InboxPage })),
);
const FleetPage = lazy(() =>
  import("../fleet/FleetPage").then((all) => ({ default: all.FleetPage })),
);
const RunPage = lazy(() => import("../runs/RunPage").then((all) => ({ default: all.RunPage })));
const TerminalPage = lazy(() =>
  import("../terminal/TerminalPage").then((all) => ({ default: all.TerminalPage })),
);

/** The routes of this area, as children of the shell's route. */
export const fleetRoutes = [
  { path: "inbox", element: <InboxPage /> },
  { path: "fleet", element: <FleetPage /> },
  { path: "fleet/runners", element: <FleetPage section="runners" /> },
  { path: "fleet/ports", element: <FleetPage section="ports" /> },
  { path: "runs/:id", element: <RunPage /> },
  { path: "terminal", element: <TerminalPage /> },
];
