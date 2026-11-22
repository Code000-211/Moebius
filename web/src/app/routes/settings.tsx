/** The routes of the app's area: what is set up once: Get started, usage, kinds of issues, tokens, plugin settings, and Settings itself with the pages the table of `nav.ts` gives an entry of its navigation. Pages are fetched when their route opens. */
import { type ComponentType, lazy } from "react";
import { SettingsLayout } from "../settings/SettingsLayout";

const named = <T extends string>(load: () => Promise<Record<T, ComponentType>>, name: T) =>
  lazy(() => load().then((all) => ({ default: all[name] })));

/** The page of each entry of the Settings navigation, by its path; the table says where the entry stands. */
const SETTINGS_PAGES: Record<string, ComponentType> = {
  profile: named(() => import("../settings/ProfilePage"), "ProfilePage"),
  appearance: named(() => import("../settings/AppearancePage"), "AppearancePage"),
  notifications: named(() => import("../settings/NotificationsPage"), "NotificationsPage"),
  shortcuts: named(() => import("../shortcuts/ShortcutsPage"), "ShortcutsPage"),
  models: named(() => import("../models/ModelsPage"), "ModelsPage"),
  plugins: named(() => import("../plugins/PluginsPage"), "PluginsPage"),
  "column-scripts": named(() => import("../settings/ColumnScriptsPage"), "ColumnScriptsPage"),
  schedules: named(() => import("../settings/SchedulesPage"), "SchedulesPage"),
  analytics: named(() => import("../analytics/AnalyticsPage"), "AnalyticsPage"),
  retro: named(() => import("../retro/RetroPage"), "RetroPage"),
  lessons: named(() => import("../settings/LessonsPage"), "LessonsPage"),
  export: named(() => import("../settings/ExportPage"), "ExportPage"),
  settings: named(() => import("../settings/SettingsPage"), "SettingsPage"),
  repositories: named(() => import("../settings/RepositoriesPage"), "RepositoriesPage"),
  people: named(() => import("../people/PeoplePage"), "PeoplePage"),
};

const KindsPage = lazy(() =>
  import("../kinds/KindsPage").then((all) => ({ default: all.KindsPage })),
);
const TokensPage = lazy(() =>
  import("../people/TokensPage").then((all) => ({ default: all.TokensPage })),
);
const PluginSettingsPage = lazy(() =>
  import("../plugins/PluginSettingsPage").then((all) => ({ default: all.PluginSettingsPage })),
);
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
    children: Object.entries(SETTINGS_PAGES).map(([path, Page]) => ({ path, element: <Page /> })),
  },
  { path: "usage", element: <UsagePage /> },
  { path: "start", element: <StartPage /> },
  { path: "kinds", element: <KindsPage /> },
  { path: "tokens", element: <TokensPage /> },
  { path: "plugin-settings", element: <PluginSettingsPage /> },
];
