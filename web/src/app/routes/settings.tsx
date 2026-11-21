/** The routes of the app's area: what is set up once: Get started, usage, kinds of issues, tokens, plugin settings, More, and Settings itself with the pages `settingsEntries.ts` registers. Pages are fetched when their route opens. */
import { lazy } from "react";
import { SettingsLayout } from "../settings/SettingsLayout";
import { SETTINGS_ENTRIES } from "../settings/settingsEntries";

const KindsPage = lazy(() =>
  import("../kinds/KindsPage").then((all) => ({ default: all.KindsPage })),
);
const TokensPage = lazy(() =>
  import("../people/TokensPage").then((all) => ({ default: all.TokensPage })),
);
const PluginSettingsPage = lazy(() =>
  import("../plugins/PluginSettingsPage").then((all) => ({ default: all.PluginSettingsPage })),
);
const MorePage = lazy(() => import("../more/MorePage").then((all) => ({ default: all.MorePage })));
const StartPage = lazy(() =>
  import("../start/StartPage").then((all) => ({ default: all.StartPage })),
);
const UsagePage = lazy(() =>
  import("../usage/UsagePage").then((all) => ({ default: all.UsagePage })),
);

/** The routes of this area, as children of the shell's route. */
export const settingsRoutes = [
  // Settings: one layout route around every page `settingsEntries.ts` registers, each at the path it always had.
  {
    element: <SettingsLayout />,
    children: SETTINGS_ENTRIES.flatMap((entry) =>
      entry.page ? [{ path: entry.path, element: <entry.page /> }] : [],
    ),
  },
  { path: "usage", element: <UsagePage /> },
  { path: "start", element: <StartPage /> },
  { path: "kinds", element: <KindsPage /> },
  { path: "tokens", element: <TokensPage /> },
  { path: "plugin-settings", element: <PluginSettingsPage /> },
  { path: "more", element: <MorePage /> },
];
