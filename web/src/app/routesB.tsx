/** The routes of the Agents, Skills, MCP servers, Models and Plugins pages; `routes.tsx` puts them before the placeholders. */
import { AgentsPage } from "./agents/AgentsPage";
import { McpPage } from "./mcp/McpPage";
import { ModelsPage } from "./models/ModelsPage";
import { PluginsPage } from "./plugins/PluginsPage";
import { SkillsPage } from "./skills/SkillsPage";

export const harnessRoutes = [
  { path: "agents", element: <AgentsPage /> },
  { path: "skills", element: <SkillsPage /> },
  { path: "mcp-servers", element: <McpPage /> },
  { path: "models", element: <ModelsPage /> },
  { path: "plugins", element: <PluginsPage /> },
];
