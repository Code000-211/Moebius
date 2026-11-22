/** The tabs of the Agents page, shared by its three pages: Agents, Skills and MCP servers, each at its own address. */
import { type SectionTab, SectionTabs } from "../shell/SectionTabs";

export const AGENT_TABS: readonly SectionTab[] = [
  { path: "agents", label: "Agents" },
  { path: "agents/skills", label: "Skills" },
  { path: "agents/mcp-servers", label: "MCP servers" },
];

export function AgentsTabs({ value }: { value: string }) {
  return <SectionTabs tabs={AGENT_TABS} value={value} label="Agents" />;
}
