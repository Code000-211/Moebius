/** The routes of the app's area: what is set up once: Get started and Settings itself with the pages the table of `nav.ts` gives an entry of its navigation. Pages are fetched when their route opens. */
import { type ComponentType, lazy } from "react";
import { SettingsLayout } from "../settings/SettingsLayout";

const named = <T extends string>(load: () => Promise<Record<T, ComponentType>>, name: T) =>
  lazy(() => load().then((all) => ({ default: all[name] })));

/** The page of each entry of the Settings navigation, by its path; the table says where the entry stands. */
const SETTINGS_PAGES: Record<string, ComponentType> = {
  profile: named(() => import("../settings/ProfilePage"), "ProfilePage"),
  appearance: named(() => import("../settings/AppearancePage"), "AppearancePage"),
  shortcuts: named(() => import("../shortcuts/ShortcutsPage"), "ShortcutsPage"),
  models: named(() => import("../models/ModelsPage"), "ModelsPage"),
  plugins: named(() => import("../plugins/PluginsPage"), "PluginsPage"),
  settings: named(() => import("../settings/SettingsPage"), "SettingsPage"),
  tokens: named(() => import("../people/TokensPage"), "TokensPage"),
  kinds: named(() => import("../kinds/KindsPage"), "KindsPage"),
  people: named(() => import("../people/PeoplePage"), "PeoplePage"),
};

const StartPage = lazy(() =>
  import("../start/StartPage").then((all) => ({ default: all.StartPage })),
);

/** The routes of this area, as children of the shell's route. */
export const settingsRoutes = [
  // Settings: one layout route around every page `settingsEntries.ts` registers, each at the path it always had.
  {
    element: <SettingsLayout />,
    children: Object.entries(SETTINGS_PAGES).map(([path, Page]) => ({ path, element: <Page /> })),
  },
  { path: "start", element: <StartPage /> },
];
