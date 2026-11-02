/** The routes of the Agents, Skills, MCP servers, Models and Plugins pages; `routes.tsx` puts them before the placeholders. They are fetched when their route opens, like every other page. */
import { lazy } from "react";

const AgentsPage = lazy(() =>
  import("./agents/AgentsPage").then((all) => ({ default: all.AgentsPage })),
);
const McpPage = lazy(() => import("./mcp/McpPage").then((all) => ({ default: all.McpPage })));
const ModelsPage = lazy(() =>
  import("./models/ModelsPage").then((all) => ({ default: all.ModelsPage })),
);
const PluginsPage = lazy(() =>
  import("./plugins/PluginsPage").then((all) => ({ default: all.PluginsPage })),
);
const SkillsPage = lazy(() =>
  import("./skills/SkillsPage").then((all) => ({ default: all.SkillsPage })),
);

export const harnessRoutes = [
  { path: "agents", element: <AgentsPage /> },
  { path: "skills", element: <SkillsPage /> },
  { path: "mcp-servers", element: <McpPage /> },
  { path: "models", element: <ModelsPage /> },
  { path: "plugins", element: <PluginsPage /> },
];
