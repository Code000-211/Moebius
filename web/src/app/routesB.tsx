/** The routes of the Agents, Skills, MCP servers, Models and Plugins pages; `routes.tsx` puts them before the placeholders. They are fetched when their route opens, like every other page. */
import { lazy } from "react";

const McpPage = lazy(() => import("./mcp/McpPage").then((all) => ({ default: all.McpPage })));

export const harnessRoutes = [{ path: "mcp-servers", element: <McpPage /> }];
