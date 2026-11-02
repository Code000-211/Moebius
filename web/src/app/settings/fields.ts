/**
 * The keys of `moebius.toml` the Settings page edits, by box, and one key as an input: what it holds, what it held when
 * read, the value a save sends, and which keys a save sets and which it removes (the desktop app's
 * `screens/setup/fields.rs` and `forms.rs`). The Models page edits the other keys.
 */
import type { ServerSettings } from "../../api/types/settings";

/** How a key of the configuration is typed. */
export type Kind =
  | "text"
  /** A path, a URL or an address, set in the code font. */
  | "code"
  | "whole"
  | "decimal"
  /** Names separated by commas. */
  | "list"
  /** True or false, sent as a JSON boolean. */
  | "yesno";

/**
 * A key of the configuration as a form shows it: the dotted key, its label, how it is typed, and — for a key with no
 * default — what an empty field means, so that empty is not a guess.
 */
export type Spec = readonly [key: string, label: string, kind: Kind, whenEmpty?: string];

export const GROUPS: ReadonlyArray<readonly [title: string, specs: readonly Spec[]]> = [
  [
    "Server",
    [
      ["data_dir", "Data directory", "code"],
      ["listen", "Desktop API listens on", "code"],
      ["token_file", "Token file", "code"],
      ["router.listen", "Router listens on", "code"],
      ["run_limit", "Runs at a time", "whole"],
    ],
  ],
  [
    "Board and budgets",
    [
      ["board.tick_ms", "Board tick (ms)", "whole"],
      ["board.round_limit", "Coding rounds a card", "whole"],
      ["budgets.day_tokens", "Tokens a day", "whole", "No limit"],
      ["checks.timeout_seconds", "Checks time limit (s)", "whole"],
    ],
  ],
  [
    "Nightly schedule",
    [
      ["retro_at", "Retro at (HH:MM)", "text"],
      ["trial_window.start", "Trial window starts", "text"],
      ["trial_window.end", "Trial window ends", "text"],
      ["regression.size", "Regression cards", "whole"],
      ["regression.gate", "Regression gate", "yesno"],
    ],
  ],
  [
    "Harness",
    [
      ["harness.url", "Repository URL", "code"],
      ["harness.mcp_env", "MCP variables", "list"],
      ["rules_file", "Rules file", "code"],
    ],
  ],
  ["Pi-Bolt", [["pi_bolt.program", "Program", "code"]]],
  [
    "GitHub",
    [
      ["github.app_id", "App id", "whole", "Set both, or neither"],
      ["github.private_key_file", "Private key file", "code", "Set both, or neither"],
      ["github.api_url", "API URL", "code"],
      ["github.poll_seconds", "Poll every (s)", "whole"],
    ],
  ],
  [
    "GitLab",
    [
      ["gitlab.url", "URL", "code"],
      ["gitlab.token_file", "Token file", "code", "This one or the secret"],
      ["gitlab.token_secret", "Token secret", "code", "This one or the file"],
      ["gitlab.poll_seconds", "Poll every (s)", "whole"],
    ],
  ],
  [
    "Forgejo",
    [
      ["forgejo.url", "URL", "code"],
      ["forgejo.token_file", "Token file", "code", "This one or the secret"],
      ["forgejo.token_secret", "Token secret", "code", "This one or the file"],
      ["forgejo.poll_seconds", "Poll every (s)", "whole"],
    ],
  ],
];

/** The tables of the file that are there or not: when every key of one is blank the table is removed. */
const TABLES: ReadonlyArray<readonly [table: string, keys: readonly string[]]> = [
  ["github", ["github.app_id", "github.private_key_file", "github.api_url", "github.poll_seconds"]],
  ["gitlab", ["gitlab.url", "gitlab.token_file", "gitlab.token_secret", "gitlab.poll_seconds"]],
  [
    "forgejo",
    ["forgejo.url", "forgejo.token_file", "forgejo.token_secret", "forgejo.poll_seconds"],
  ],
  ["harness", ["harness.url", "harness.mcp_env"]],
];

/** One key of the configuration as an input. */
export interface Field {
  key: string;
  label: string;
  kind: Kind;
  /** What the input holds; blank when the file does not set the key. */
  text: string;
  /** What it held when the settings were read. */
  original: string;
  /** What the server uses when the file does not set the key. */
  default: string | null;
  /** For a key with no default: what leaving the field blank means, in a few words. */
  whenEmpty: string | null;
}

/** A form: the inputs of the keys of one box. */
export interface Form {
  title: string;
  fields: Field[];
}

/** A value as the text of an input. */
function textOf(value: unknown): string {
  if (value === null || value === undefined) return "";
  if (typeof value === "string") return value;
  if (Array.isArray(value)) return value.map(textOf).join(", ");
  return String(value);
}

export function readField(spec: Spec, settings: ServerSettings): Field {
  const [key, label, kind, whenEmpty] = spec;
  const setting = settings.values[key];
  const shown = textOf(setting?.value);
  const set = setting !== undefined && !setting.default;
  // A picker always shows a choice, so it holds the default when the file does not set the key.
  const text = set || kind === "yesno" ? shown : "";
  return {
    key,
    label,
    kind,
    text,
    original: text,
    default: !set && shown !== "" ? shown : null,
    whenEmpty: whenEmpty ?? null,
  };
}

export function readForms(settings: ServerSettings): Form[] {
  return GROUPS.map(([title, specs]) => ({
    title,
    fields: specs.map((spec) => readField(spec, settings)),
  }));
}

/** The new value of the key, null when the input is blank; or what is wrong with the input. */
export function fieldValue(field: Field): { value: unknown } | { error: string } {
  const text = field.text.trim();
  if (text === "") return { value: null };
  const wrong = (what: string) => ({ error: `${field.label} is ${what}.` });
  switch (field.kind) {
    case "yesno":
      return { value: text === "true" };
    case "text":
    case "code":
      return { value: text };
    case "whole":
      return /^-?\d+$/.test(text) ? { value: Number(text) } : wrong("a whole number");
    case "decimal":
      return text !== "" && Number.isFinite(Number(text))
        ? { value: Number(text) }
        : wrong("a number");
    case "list":
      return {
        value: text
          .split(",")
          .map((name) => name.trim())
          .filter((name) => name !== ""),
      };
  }
}

export const changed = (field: Field): boolean => field.text.trim() !== field.original.trim();

const wasSet = (field: Field): boolean => field.original.trim() !== "";

/** What a save sends. */
export interface Change {
  set: Record<string, unknown>;
  remove: string[];
}

/** The keys the page changed: set when they hold a value, removed when blank; a table whose inputs are all blank goes as a whole. */
export function changes(forms: readonly Form[]): Change | { error: string } {
  const set: Record<string, unknown> = {};
  let remove: string[] = [];
  const fields = forms.flatMap((form) => form.fields);
  for (const field of fields.filter(changed)) {
    const parsed = fieldValue(field);
    if ("error" in parsed) return parsed;
    if (parsed.value === null) remove.push(field.key);
    else set[field.key] = parsed.value;
  }
  for (const [table, keys] of TABLES) {
    const own = fields.filter((field) => keys.includes(field.key));
    if (own.every((field) => field.text.trim() === "") && own.some(wasSet)) {
      remove = remove.filter((key) => !keys.includes(key));
      remove.push(table);
    }
  }
  return { set, remove };
}

/** Whether a save has nothing to send. */
export const nothingToSave = (change: Change): boolean =>
  Object.keys(change.set).length === 0 && change.remove.length === 0;
