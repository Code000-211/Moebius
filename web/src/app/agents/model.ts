/**
 * The logic of the Agents page, free of React: the fields of the editor, what a save sends (or what is wrong with the
 * fields), how an agent of `moebius.toml` is read from the server's settings and written back as a table, and the
 * choices of the Model field. It follows `crates/moebius-desktop/src/screens/agents/editor.rs`.
 */
import type { Agent, AvailableClass, AvailableModel, AvailableModels } from "../../api/types/chat";
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
/** The engineering skills Moebius brings, which any agent can be given, with what each is for. */
export const ENGINEERING_SKILLS = [
  { name: "tdd", line: "Work test first: red, green, refactor." },
  { name: "diagnosing-bugs", line: "Reproduce a bug, find its cause and fix that." },
  { name: "code-review", line: "Review a change against the issue and the standards." },
  { name: "grilling", line: "Question an idea until the work is clear." },
  { name: "to-tickets", line: "Cut the work into small issues." },
  {
    name: "moebius-autonomous",
    line: "How to use the others with no person to ask.",
  },
];

/** The engineering skills an agent of this name is given when its setting is left at the default. */
export function engineeringDefaults(name: string): string[] {
  switch (name) {
    case "Coder":
      return ["tdd", "diagnosing-bugs", "moebius-autonomous"];
    case "Reviewer":
      return ["code-review", "moebius-autonomous"];
    case "Grilling-partner":
      return ["grilling", "to-tickets", "moebius-autonomous"];
    default:
      return [];
  }
}

/** The engineering skills the agent has: its list, or the defaults of its name while the list is not its own. */
export function engineeringOf(form: AgentForm): string[] {
  return form.engineeringSkills ?? engineeringDefaults(form.name);
}

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
  /** The engineering skills it is given, exactly; null leaves them at the defaults of its name. */
  engineeringSkills: string[] | null;
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
    engineeringSkills: null,
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
    engineeringSkills: Array.isArray(settings.engineering_skills)
      ? settings.engineering_skills
      : settings.engineering_skills === false
        ? []
        : null,
    instructions: detail.instructions ?? "",
    skills: settings.skills ?? [],
    mcp: settings.mcp ?? [],
    extensions: settings.extensions ?? [],
  };
}

/** The built-in agents a new agent can start from; the others Moebius runs for itself are no template. */
const COPYABLE_BUILT_IN = ["Coder", "Reviewer"];

/** The agents a new agent can start from: the Coder and the Reviewer, which always exist, and the person's own. */
export function copyable(agents: Agent[]): Agent[] {
  return agents.filter(
    (agent) =>
      agent.error === undefined && (!agent.built_in || COPYABLE_BUILT_IN.includes(agent.name)),
  );
}

/**
 * The editor of a new agent that starts from the agent `detail` describes: all its settings and instructions, under a
 * name still to be typed, and not built in. A built-in agent's own settings are those it runs with (`listed`, from
 * `GET /v1/agents`) when it has no files, and its instructions are the built-in ones followed by its own, which the
 * copy keeps as its own.
 */
export function copyOf(detail: AgentDetail, listed: Agent | undefined): AgentForm {
  const form = formOf({ ...detail, built_in: undefined });
  if (!detail.settings && listed) {
    form.model = listed.model;
    form.protocol = listed.protocol;
    form.harness = listed.harness;
    form.thinking = listed.thinking ?? null;
    form.budget = listed.token_budget === undefined ? "" : String(listed.token_budget);
  }
  // The copy has another name, so it is given the skills the source has as a list of its own.
  if (form.engineeringSkills === null) {
    const given =
      detail.built_in?.engineering_skills?.map((skill) => skill.name) ??
      engineeringDefaults(detail.name);
    if (given.length > 0) form.engineeringSkills = given;
  }
  const own = detail.instructions ?? "";
  const builtIn = detail.built_in?.instructions;
  form.instructions =
    builtIn === undefined ? own : own.trim() === "" ? builtIn : `${builtIn}\n\n${own}`;
  return { ...form, name: "", naming: true, exists: false, builtIn: null };
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
  if (form.engineeringSkills !== null) {
    settings.engineering_skills =
      form.engineeringSkills.length === 0 ? false : form.engineeringSkills;
  }
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
  if (Array.isArray(table.engineering_skills)) {
    settings.engineering_skills = table.engineering_skills.filter((v) => typeof v === "string");
  }
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
  if (settings.engineering_skills !== undefined && settings.engineering_skills !== true) {
    table.engineering_skills = settings.engineering_skills;
  }
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
