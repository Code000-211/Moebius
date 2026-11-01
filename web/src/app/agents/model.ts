/**
 * The logic of the Agents page, free of React: the fields of the editor, what a save sends (or what is wrong with the
 * fields), how an agent of `moebius.toml` is read from the server's settings and written back as a table, and the
 * choices of the Model field. It follows `crates/moebius-desktop/src/screens/agents/editor.rs`.
 */
import type { AvailableClass, AvailableModel, AvailableModels } from "../../api/types/chat";
import {
  type AgentDetail,
  type AgentSettings,
  type BuiltInAgent,
  DEFAULT_TOOL_OUTPUT,
  type ToolOutput,
} from "../../api/types/harness";
import type { ServerSettings } from "../../api/types/models";

export const PROTOCOLS = ["openai", "anthropic"] as const;
export const HARNESSES = ["pi-bolt", "claude-code"] as const;
/** What the thinking list says for no level at all. */
export const NOT_SET = "Not set";
/** What the Runner list says for the server itself. */
export const THIS_SERVER = "This server";
/** The agents Moebius brings engineering skills for. */
export const ENGINEERING_AGENTS = ["Coder", "Reviewer", "Grilling-partner"];

/** What the editor holds: every field as the person sees it. */
export interface AgentForm {
  name: string;
  /** Whether the name is still being typed, as for a new agent. */
  naming: boolean;
  /** Whether the agent has files, so that it can be deleted. */
  exists: boolean;
  model: string;
  protocol: string;
  harness: string;
  runner: string | null;
  thinking: string | null;
  budget: string;
  hideAfter: string;
  maxLines: string;
  maxBytes: string;
  compress: boolean;
  engineeringSkills: boolean;
  instructions: string;
  skills: string[];
  mcp: string[];
  extensions: string[];
  /** What a built-in agent brings itself, shown read only. */
  builtIn: BuiltInAgent | null;
}

function toolOutputFields(output: ToolOutput) {
  return {
    hideAfter: String(output.hide_after_turns),
    maxLines: String(output.max_lines),
    maxBytes: String(output.max_bytes),
    compress: output.compress,
  };
}

export function blankForm(name: string, naming = false): AgentForm {
  return {
    name,
    naming,
    exists: false,
    model: "",
    protocol: PROTOCOLS[0],
    harness: HARNESSES[0],
    runner: null,
    thinking: null,
    budget: "",
    ...toolOutputFields(DEFAULT_TOOL_OUTPUT),
    engineeringSkills: true,
    instructions: "",
    skills: [],
    mcp: [],
    extensions: [],
    builtIn: null,
  };
}

/** The editor of the agent `detail` describes. A built-in agent without files is edited from blank. */
export function formOf(detail: AgentDetail): AgentForm {
  const form = blankForm(detail.name);
  form.builtIn = detail.built_in ?? null;
  const settings = detail.settings;
  if (!settings) return form;
  return {
    ...form,
    exists: true,
    model: settings.model,
    protocol: settings.protocol,
    harness: settings.harness ?? HARNESSES[0],
    runner: settings.runner ?? null,
    thinking: settings.thinking ?? null,
    budget: settings.token_budget === undefined ? "" : String(settings.token_budget),
    ...toolOutputFields(settings.tool_output ?? DEFAULT_TOOL_OUTPUT),
    engineeringSkills: settings.engineering_skills ?? true,
    instructions: detail.instructions ?? "",
    skills: settings.skills ?? [],
    mcp: settings.mcp ?? [],
    extensions: settings.extensions ?? [],
  };
}

export interface AgentRequest {
  name: string;
  settings: AgentSettings;
  instructions: string;
}

const NAME = /^[A-Za-z0-9_-]+$/;
const WHOLE = /^\d+$/;

/** What a save sends for the fields, or what is wrong with them. */
export function agentRequest(form: AgentForm): AgentRequest | string {
  const name = form.name.trim();
  if (!NAME.test(name)) return "A name is made of letters, digits, - and _.";
  const model = form.model.trim();
  if (model === "") return "Enter the model.";
  let tokenBudget: number | undefined;
  const budget = form.budget.trim();
  if (budget !== "") {
    tokenBudget = WHOLE.test(budget) ? Number(budget) : Number.NaN;
    if (!(tokenBudget >= 1)) return "The token budget is a whole number of at least 1.";
  }
  const numbers = [form.hideAfter, form.maxLines, form.maxBytes].map((field) => field.trim());
  if (!numbers.every((field) => WHOLE.test(field) && Number(field) <= 4294967295)) {
    return "The tool output fields are whole numbers; 0 turns one off.";
  }
  const [hide, lines, bytes] = numbers.map(Number) as [number, number, number];
  const toolOutput: ToolOutput = {
    hide_after_turns: hide,
    max_lines: lines,
    max_bytes: bytes,
    compress: form.compress,
  };
  const settings: AgentSettings = {
    model,
    protocol: form.protocol,
    skills: form.skills,
    extensions: form.extensions,
    mcp: form.mcp,
  };
  if (form.thinking !== null) settings.thinking = form.thinking;
  if (tokenBudget !== undefined) settings.token_budget = tokenBudget;
  if (form.harness !== HARNESSES[0]) settings.harness = form.harness;
  if (form.runner !== null) settings.runner = form.runner;
  if (!sameToolOutput(toolOutput, DEFAULT_TOOL_OUTPUT)) settings.tool_output = toolOutput;
  if (!form.engineeringSkills) settings.engineering_skills = false;
  // The file is not given a second final line break.
  const instructions = form.instructions.endsWith("\n")
    ? form.instructions.slice(0, -1)
    : form.instructions;
  return { name, settings, instructions };
}

function sameToolOutput(a: ToolOutput, b: ToolOutput): boolean {
  return (
    a.hide_after_turns === b.hide_after_turns &&
    a.max_lines === b.max_lines &&
    a.max_bytes === b.max_bytes &&
    a.compress === b.compress
  );
}

/** `name` taken out of `chosen` when it is there, put in when it is not. */
export function toggle(chosen: string[], name: string): string[] {
  return chosen.includes(name) ? chosen.filter((held) => held !== name) : [...chosen, name];
}

/** `names` and what the agent has chosen that is not on offer, so that it can be taken off. */
export function withChosen(names: string[], chosen: string[]): string[] {
  return [...names, ...chosen.filter((name) => !names.includes(name))];
}

/** Adds `path` to `paths` unless it is blank or there already. */
export function addPath(paths: string[], path: string): string[] {
  const trimmed = path.trim();
  return trimmed === "" || paths.includes(trimmed) ? paths : [...paths, trimmed];
}

/** The agent `name` as the `[agents.<name>]` table of `moebius.toml` says, or null when the file has none. */
export function fromFile(name: string, server: ServerSettings): AgentDetail | null {
  const agents = server.values.agents?.value;
  const table =
    agents !== null && typeof agents === "object"
      ? (agents as Record<string, Record<string, unknown>>)[name]
      : undefined;
  if (!table) return null;
  const text = (key: string) =>
    typeof table[key] === "string" ? (table[key] as string) : undefined;
  const paths = (key: string) =>
    Array.isArray(table[key]) ? (table[key] as unknown[]).filter((v) => typeof v === "string") : [];
  const output = table.tool_output;
  const settings: AgentSettings = {
    model: text("model") ?? "",
    protocol: text("protocol") ?? PROTOCOLS[0],
    skills: paths("skills") as string[],
    extensions: paths("extensions") as string[],
    mcp: [],
  };
  const thinking = text("thinking");
  if (thinking !== undefined) settings.thinking = thinking;
  if (typeof table.token_budget === "number") settings.token_budget = table.token_budget;
  const harness = text("harness");
  if (harness !== undefined) settings.harness = harness;
  const runner = text("runner");
  if (runner !== undefined) settings.runner = runner;
  if (output !== null && typeof output === "object") {
    settings.tool_output = { ...DEFAULT_TOOL_OUTPUT, ...(output as Partial<ToolOutput>) };
  }
  if (table.engineering_skills === false) settings.engineering_skills = false;
  return { name, settings, instructions: text("instructions") ?? "" };
}

/** The `[agents.<name>]` table that `settings` and `instructions` are saved as. */
export function fileTable(settings: AgentSettings, instructions: string): Record<string, unknown> {
  const table: Record<string, unknown> = { model: settings.model, protocol: settings.protocol };
  if (settings.thinking !== undefined) table.thinking = settings.thinking;
  if (settings.harness !== undefined) table.harness = settings.harness;
  if (settings.runner !== undefined) table.runner = settings.runner;
  if (instructions.trim() !== "") table.instructions = instructions;
  if (settings.token_budget !== undefined) table.token_budget = settings.token_budget;
  if (settings.tool_output) table.tool_output = settings.tool_output;
  if (settings.engineering_skills === false) table.engineering_skills = false;
  if (settings.skills.length > 0) table.skills = settings.skills;
  if (settings.extensions.length > 0) table.extensions = settings.extensions;
  return table;
}

/** A model to choose in a Model field: a model class, or a model an upstream serves. */
export interface ModelChoice {
  id: string;
  protocol: string;
  upstream: string;
  class: boolean;
}

/** The choices of a Model field: the model classes first, then the models the upstreams serve. */
export function modelChoices(available: AvailableModels | undefined): ModelChoice[] {
  if (!available) return [];
  const classes = available.classes.map((each: AvailableClass) => ({
    id: each.name,
    protocol: each.protocols.length === 1 ? (each.protocols[0] as string) : "",
    upstream: "",
    class: true,
  }));
  return [
    ...classes,
    ...available.models.map((model: AvailableModel) => ({ ...model, class: false })),
  ];
}

/** What the decision model's Model field offers: the models its endpoint lists, or, before it lists any, the upstreams'. */
export function decisionChoices(
  available: AvailableModels | undefined,
  url: string,
): ModelChoice[] {
  if (!available) return [];
  if (available.decision_models.length === 0) {
    return available.models.map((model) => ({ ...model, class: false }));
  }
  return available.decision_models.map((id) => ({
    id,
    protocol: "openai",
    upstream: url,
    class: false,
  }));
}

/** How a choice reads in the list. */
export function choiceLabel(choice: ModelChoice): string {
  return choice.class ? `${choice.id} (class)` : choice.id;
}
