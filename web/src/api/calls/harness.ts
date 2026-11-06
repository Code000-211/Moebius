/**
 * The calls about the harness configuration: agents, skills, a skill's files, MCP servers, uploads and setting up the
 * repository (`client/calls/harness.rs` of the former desktop app). An edit is a commit on the server.
 */
import { del, get, post, put, request } from "../http";
import type {
  AgentDetail,
  AgentSettings,
  FileDetail,
  McpDefinition,
  McpServer,
  McpServerDetail,
  Runner,
  SkillDetail,
  SkillListed,
  Upload,
} from "../types/harness";
import type { SavedSettings } from "../types/models";

/** A path with its segments encoded and its slashes kept, for the `{*path}` of a route. */
export function encodedPath(path: string): string {
  return path.split("/").map(encodeURIComponent).join("/");
}

export function runners(): Promise<Runner[]> {
  return get<Runner[]>("/v1/runners");
}

export function agent(name: string): Promise<AgentDetail> {
  return get<AgentDetail>(`/v1/agents/${encodeURIComponent(name)}`);
}

export function saveAgent(
  name: string,
  settings: AgentSettings,
  instructions: string,
): Promise<AgentDetail> {
  return put<AgentDetail>(`/v1/agents/${encodeURIComponent(name)}`, { settings, instructions });
}

export async function deleteAgent(name: string): Promise<void> {
  await del<unknown>(`/v1/agents/${encodeURIComponent(name)}`);
}

export function skillList(): Promise<SkillListed[]> {
  return get<SkillListed[]>("/v1/skills");
}

export function skill(name: string): Promise<SkillDetail> {
  return get<SkillDetail>(`/v1/skills/${encodeURIComponent(name)}`);
}

export function saveSkill(name: string, description: string, text: string): Promise<SkillDetail> {
  return put<SkillDetail>(`/v1/skills/${encodeURIComponent(name)}`, { description, text });
}

export async function deleteSkill(name: string): Promise<void> {
  await del<unknown>(`/v1/skills/${encodeURIComponent(name)}`);
}

const skillFilePath = (name: string, path: string) =>
  `/v1/skills/${encodeURIComponent(name)}/files/${encodedPath(path)}`;

export function skillFile(name: string, path: string): Promise<FileDetail> {
  return get<FileDetail>(skillFilePath(name, path));
}

export function saveSkillFile(name: string, path: string, text: string): Promise<SkillDetail> {
  return put<SkillDetail>(skillFilePath(name, path), { text });
}

export function deleteSkillFile(name: string, path: string): Promise<SkillDetail> {
  return del<SkillDetail>(skillFilePath(name, path));
}

export function mcpServers(): Promise<McpServer[]> {
  return get<McpServer[]>("/v1/mcp-servers");
}

export function mcpServer(name: string): Promise<McpServerDetail> {
  return get<McpServerDetail>(`/v1/mcp-servers/${encodeURIComponent(name)}`);
}

export function saveMcpServer(name: string, definition: McpDefinition): Promise<McpServerDetail> {
  return put<McpServerDetail>(`/v1/mcp-servers/${encodeURIComponent(name)}`, definition);
}

export async function deleteMcpServer(name: string): Promise<void> {
  await del<unknown>(`/v1/mcp-servers/${encodeURIComponent(name)}`);
}

/** Sends a zip file to the server, which keeps it where the Moebius agent can read it. */
export function upload(zip: Blob): Promise<Upload> {
  return request<Upload>("POST", "/v1/uploads", {
    fetch: (path, init) =>
      fetch(path, {
        ...init,
        headers: {
          ...(init?.headers as Record<string, string>),
          "Content-Type": "application/zip",
        },
        body: zip,
      }),
  });
}

/** Moves the agents and rules of `moebius.toml` into a new harness configuration repository. */
export function setUpHarness(): Promise<SavedSettings> {
  return post<SavedSettings>("/v1/harness/setup", {});
}
