/**
 * Types of the harness configuration: agents, skills, MCP servers and uploads, as `docs/reference/desktop-api.md`
 * describes `GET /v1/agents/{name}`, `/v1/skills`, `/v1/mcp-servers` and the routes next to them. The list entries of
 * agents and skills (`Agent`, `Skill`) are in `chat.ts`, where the chat page first needed them.
 */

/** The settings of an agent as `PUT /v1/agents/{name}` takes them. */
export interface AgentSettings {
  model: string;
  protocol: string;
  thinking?: string;
  token_budget?: number;
  skills: string[];
  extensions: string[];
  mcp: string[];
  /** Left out for `pi-bolt`. */
  harness?: string;
  /** Left out when it is the defaults. */
  runner?: string;
  /** Left out when true (the defaults of the agent's name); false is none; a list is exactly those skills. */
  engineering_skills?: boolean | string[];
}

export interface BuiltInSkill {
  name: string;
  description: string;
}

/** What a built-in agent is always given, read only. */
export interface BuiltInAgent {
  instructions: string;
  skills: BuiltInSkill[];
  engineering_skills?: BuiltInSkill[];
}

/** `GET /v1/agents/{name}`; a built-in agent without files has no `settings` and no `instructions`. */
export interface AgentDetail {
  name: string;
  settings?: AgentSettings;
  instructions?: string;
  built_in?: BuiltInAgent;
  /** The short id of the commit a `PUT` made; left out when nothing changed. */
  commit?: string;
}

/** A machine that runs harnesses over SSH, `GET /v1/runners`. */
export interface Runner {
  name: string;
  host: string;
  user: string | null;
  max_runs: number;
  busy: number;
}

/** Where an engineering skill was copied from. */
export interface SkillSource {
  url: string;
  commit: string;
}

/** The skills of the list, with the source of an engineering skill. */
export interface SkillListed {
  name: string;
  description: string;
  built_in: boolean;
  source?: SkillSource;
}

export interface SkillDetail {
  name: string;
  description: string;
  text: string;
  files: string[];
  source?: SkillSource;
  commit?: string;
}

/** One of a skill's other files; `binary` or `too_large` instead of `text` for a file that cannot be edited. */
export interface FileDetail {
  path: string;
  size: number;
  text?: string;
  binary?: boolean;
  too_large?: boolean;
}

export interface McpServer {
  name: string;
  kind: string;
  error?: string;
}

/** An MCP server as `mcp.toml` has it: a command or a URL, with the environment or the headers. */
export interface McpDefinition {
  command?: string;
  args?: string[];
  env?: Record<string, string>;
  url?: string;
  headers?: Record<string, string>;
}

export type McpServerDetail = McpDefinition & { name: string; commit?: string };

/** `POST /v1/uploads`: where the server keeps the zip file. */
export interface Upload {
  id: string;
  path: string;
}
