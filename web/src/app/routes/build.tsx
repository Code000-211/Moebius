/** The routes of the app's area: Build: the Agents page with its Skills and MCP servers tabs, the page of one agent, and Rules. Pages are fetched when their route opens. */
import { lazy } from "react";

const AgentsPage = lazy(() =>
  import("../agents/AgentsPage").then((all) => ({ default: all.AgentsPage })),
);
const AgentPage = lazy(() =>
  import("../agents/AgentPage").then((all) => ({ default: all.AgentPage })),
);
const SkillsPage = lazy(() =>
  import("../skills/SkillsPage").then((all) => ({ default: all.SkillsPage })),
);
const McpPage = lazy(() => import("../mcp/McpPage").then((all) => ({ default: all.McpPage })));
const RulesPage = lazy(() =>
  import("../rules/RulesPage").then((all) => ({ default: all.RulesPage })),
);

/** The routes of this area, as children of the shell's route. */
export const buildRoutes = [
  { path: "agents", element: <AgentsPage /> },
  { path: "agents/new", element: <AgentPage creating /> },
  { path: "agents/:name", element: <AgentPage /> },
  { path: "agents/skills", element: <SkillsPage /> },
  { path: "agents/mcp-servers", element: <McpPage /> },
  { path: "rules", element: <RulesPage /> },
];
