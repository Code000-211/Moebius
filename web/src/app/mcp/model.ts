/**
 * The logic of the MCP servers page, free of React: the fields of the editor, what a save sends (or what is wrong with
 * the fields) and how a server of `mcp.toml` fills the editor (`crates/moebius-desktop/src/screens/mcp_servers.rs`).
 */
import type { McpDefinition } from "../../api/types/harness";

/** The environment of a command, or the headers of a URL: name and value. */
export type Row = [string, string];

export interface McpForm {
  name: string;
  /** Whether the server is a remote one, with a URL, and not a command. */
  remote: boolean;
  command: string;
  /** The arguments, one per line. */
  args: string;
  url: string;
  rows: Row[];
}

export const blankForm = (name = ""): McpForm => ({
  name,
  remote: false,
  command: "",
  args: "",
  url: "",
  rows: [],
});

/** The editor of the server `definition` describes. */
export function formOf(name: string, definition: McpDefinition): McpForm {
  const remote = definition.url !== undefined;
  return {
    name,
    remote,
    command: definition.command ?? "",
    args: (definition.args ?? []).join("\n"),
    url: definition.url ?? "",
    rows: Object.entries(remote ? (definition.headers ?? {}) : (definition.env ?? {})),
  };
}

const NAME = /^[A-Za-z0-9_-]+$/;

/** The server's name and what a `PUT` sends for it, or what is wrong with the fields. */
export function mcpRequest(form: McpForm): { name: string; definition: McpDefinition } | string {
  const name = form.name.trim();
  if (!NAME.test(name)) return "A name is made of letters, digits, - and _.";
  const values: Record<string, string> = {};
  for (const [key, value] of form.rows) {
    if (key.trim() === "") {
      if (value.trim() === "") continue;
      return "Give each value a name.";
    }
    values[key.trim()] = value;
  }
  if (form.remote) {
    if (form.url.trim() === "") return "Enter the URL.";
    const definition: McpDefinition = { url: form.url.trim() };
    if (Object.keys(values).length > 0) definition.headers = values;
    return { name, definition };
  }
  if (form.command.trim() === "") return "Enter the command.";
  const definition: McpDefinition = { command: form.command.trim() };
  const args = form.args.split("\n").filter((line) => line.trim() !== "");
  if (args.length > 0) definition.args = args;
  if (Object.keys(values).length > 0) definition.env = values;
  return { name, definition };
}
