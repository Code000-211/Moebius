/**
 * The logic of the Models page, free of React: the forms made from `GET /v1/settings` (the upstreams with their API
 * keys, the model classes, the teacher and the decision model, either of which may be left out) and what a save sends:
 * the keys to store by name, then the dotted keys to set and to remove in `moebius.toml`. It follows
 * `crates/moebius-desktop/src/screens/setup/{fields,forms,provider}.rs`.
 */
import type { ServerSettings } from "../../api/types/models";

export const PROTOCOLS = ["openai", "anthropic"] as const;
export const FLOWS = ["code", "device"] as const;

export type Json = unknown;
type Obj = Record<string, Json>;

/** What a save sends. */
export interface Plan {
  /** API keys to store by name; they go first, since the file names them and the server refuses a name it does not have. */
  store: Array<[string, string]>;
  set: Obj;
  remove: string[];
}

const isObject = (value: Json): value is Obj =>
  value !== null && typeof value === "object" && !Array.isArray(value);

const same = (a: Json, b: Json) => JSON.stringify(a) === JSON.stringify(b);

const setting = (server: ServerSettings, key: string): Json => server.values[key]?.value ?? null;
const isSet = (server: ServerSettings, key: string) => {
  const found = server.values[key];
  return found !== undefined && !found.default;
};

// --- the keys of the file as inputs -----------------------------------------------------------------------------

export type Kind = "text" | "model" | "code" | "whole" | "decimal" | "list" | "pick";
export interface Spec {
  key: string;
  label: string;
  kind: Kind;
}

export const TEACHER: Spec[] = [
  { key: "teacher.model", label: "Model", kind: "model" },
  { key: "teacher.protocol", label: "Protocol", kind: "pick" },
  { key: "teacher.timeout_seconds", label: "Time limit (s)", kind: "whole" },
];

export const DECISION_MODEL: Spec[] = [
  { key: "decision_model.url", label: "URL", kind: "code" },
  { key: "decision_model.model", label: "Model", kind: "model" },
  { key: "decision_model.timeout_seconds", label: "Time limit (s)", kind: "whole" },
  { key: "decision_model.shadow", label: "Kinds to shadow", kind: "list" },
  { key: "decision_model.agreement", label: "Agreement needed", kind: "decimal" },
  { key: "decision_model.min_judgements", label: "Judgements needed", kind: "whole" },
];

/** One key of the configuration as an input: what it holds, what it held when read, and the file's default. */
export interface Field extends Spec {
  text: string;
  original: string;
  /** What the server uses when the file does not set the key. */
  default: string | null;
}

/** A value as the text of an input. */
export function textOf(value: Json): string {
  if (value === null || value === undefined) return "";
  if (typeof value === "string") return value;
  if (Array.isArray(value)) return value.map(textOf).join(", ");
  return JSON.stringify(value);
}

function readField(spec: Spec, server: ServerSettings): Field {
  const shown = textOf(setting(server, spec.key));
  const set = isSet(server, spec.key);
  // A picker always shows a choice, so it holds the default when the file does not set the key.
  const text = set || spec.kind === "pick" ? shown : "";
  return { ...spec, text, original: text, default: !set && shown !== "" ? shown : null };
}

/** The new value of a field, null when the input is blank; or what is wrong with the input. */
export function fieldValue(field: Field): { value: Json | null } | { problem: string } {
  const text = field.text.trim();
  if (text === "") return { value: null };
  switch (field.kind) {
    case "whole":
      return /^[+-]?\d+$/.test(text)
        ? { value: Number(text) }
        : { problem: `${field.label} is a whole number.` };
    case "decimal":
      return Number.isFinite(Number(text))
        ? { value: Number(text) }
        : { problem: `${field.label} is a number.` };
    case "list":
      return {
        value: text
          .split(",")
          .map((name) => name.trim())
          .filter((name) => name !== ""),
      };
    default:
      return { value: text };
  }
}

const changed = (field: Field) => field.text.trim() !== field.original.trim();

function fieldChanges(fields: Field[], plan: Plan): string | null {
  for (const field of fields.filter(changed)) {
    const result = fieldValue(field);
    if ("problem" in result) return result.problem;
    if (result.value === null) plan.remove.push(field.key);
    else plan.set[field.key] = result.value;
  }
  return null;
}

function allFields(fields: Field[], plan: Plan): string | null {
  for (const field of fields) {
    const result = fieldValue(field);
    if ("problem" in result) return result.problem;
    if (result.value !== null) plan.set[field.key] = result.value;
  }
  return null;
}

// --- API keys ----------------------------------------------------------------------------------------------------

export type KeyMode = "none" | "env" | "typed";

export const KEY_MODES: Array<[KeyMode, string]> = [
  ["none", "No key"],
  ["env", "From the environment"],
  ["typed", "Typed key"],
];

/** Where the key of an upstream or of the decision model comes from. */
export interface KeyChoice {
  mode: KeyMode;
  /** The environment variable. */
  env: string;
  /** The name of the key the server stores, if it has one for this. */
  stored: string | null;
  /** The key being typed; never read back from the server. */
  typed: string;
}

const nameOf = (value: Json) => (typeof value === "string" ? value : null);

export function readKey(env: Json, secret: Json): KeyChoice {
  const [variable, stored] = [nameOf(env), nameOf(secret)];
  return {
    mode: variable !== null ? "env" : stored !== null ? "typed" : "none",
    env: variable ?? "",
    stored,
    typed: "",
  };
}

/**
 * The key to send: the environment variable or the name of the stored key, which is `fresh()` when none is stored yet;
 * and the key to store under that name, if one was typed.
 */
function planKey(
  key: KeyChoice,
  what: string,
  fresh: () => string,
): { keys: Obj; store: [string, string] | null } | string {
  const keys: Obj = {};
  let store: [string, string] | null = null;
  if (key.mode === "env") {
    if (key.env.trim() === "") return `Enter the environment variable of the key of ${what}.`;
    keys.api_key_env = key.env.trim();
  } else if (key.mode === "typed") {
    const typed = key.typed.trim();
    if (typed === "" && key.stored === null) return `Type the key of ${what}.`;
    const name = key.stored ?? fresh();
    if (typed !== "") store = [name, typed];
    keys.api_key_secret = name;
  }
  return { keys, store };
}

// --- upstreams, classes and the optional tables ------------------------------------------------------------------

let uids = 0;
/** A number that names a row of a list for as long as the page lives, so that removing one keeps the others' fields. */
export function nextUid(): number {
  uids += 1;
  return uids;
}

export interface UpstreamForm {
  uid: number;
  /** The place in the list the file had it, null for one added here. */
  origin: number | null;
  protocol: string;
  baseUrl: string;
  /** The `oauth` table of an upstream that logs in with OAuth, kept as the file has it. */
  oauth: Json | null;
  key: KeyChoice;
}

export const blankUpstream = (): UpstreamForm => ({
  uid: nextUid(),
  origin: null,
  protocol: PROTOCOLS[0],
  baseUrl: "",
  oauth: null,
  key: { mode: "none", env: "", stored: null, typed: "" },
});

function readUpstream(value: Json, origin: number): UpstreamForm {
  const table = isObject(value) ? value : {};
  const text = (name: string) => (typeof table[name] === "string" ? (table[name] as string) : "");
  return {
    uid: nextUid(),
    origin,
    protocol: text("protocol"),
    baseUrl: text("base_url"),
    oauth: table.oauth ?? null,
    key: readKey(table.api_key_env, table.api_key_secret),
  };
}

/** A model class of `router.classes`: its name and its models, which a person types separated by commas. */
export interface ClassForm {
  uid: number;
  name: string;
  models: string;
  /** The day's tokens of its models that have them in the file, kept while the model stays in the class. */
  dayTokens: Array<[string, number]>;
}

function readClass(name: string, value: Json): ClassForm {
  const models: string[] = [];
  const dayTokens: Array<[string, number]> = [];
  for (const entry of Array.isArray(value) ? value : []) {
    const model =
      typeof entry === "string"
        ? entry
        : isObject(entry) && typeof entry.model === "string"
          ? entry.model
          : "";
    if (isObject(entry) && typeof entry.day_tokens === "number")
      dayTokens.push([model, entry.day_tokens]);
    models.push(model);
  }
  return { uid: nextUid(), name, models: models.join(", "), dayTokens };
}

/** The models as the file writes them: a name, or a table with the day's tokens the model had. */
function classEntries(form: ClassForm): Json[] {
  return form.models
    .split(",")
    .map((model) => model.trim())
    .filter((model) => model !== "")
    .map((model) => {
      const found = form.dayTokens.find(([name]) => name === model);
      return found ? { model, day_tokens: found[1] } : model;
    });
}

/** The teacher or the decision model: a table of the file that can be turned off. */
export interface OptionalForm {
  wasOn: boolean;
  on: boolean;
  fields: Field[];
  /** Only the decision model has a key: what it is now, and what it was when read. */
  key: { now: KeyChoice; before: KeyChoice } | null;
}

function optionalChanges(
  form: OptionalForm,
  table: string,
  plan: Plan,
  fresh: () => string,
): string | null {
  if (form.wasOn && !form.on) {
    plan.remove.push(table);
    return null;
  }
  if (!form.on) return null;
  const isNew = !form.wasOn;
  const keyChanged =
    form.key !== null &&
    (isNew || !same(form.key.now, form.key.before) || form.key.now.typed !== "");
  if (!isNew && !form.fields.some(changed) && !keyChanged) return null;
  const model = form.fields.find((field) => field.key === `${table}.model`);
  if (model && model.text.trim() === "")
    return `Enter the model of the ${table.replaceAll("_", " ")}.`;
  const problem = isNew ? allFields(form.fields, plan) : fieldChanges(form.fields, plan);
  if (problem) return problem;
  if (form.key !== null && keyChanged) {
    const planned = planKey(form.key.now, "the decision model", fresh);
    if (typeof planned === "string") return planned;
    if (planned.store) plan.store.push(planned.store);
    for (const name of ["api_key_env", "api_key_secret"]) {
      if (name in planned.keys) plan.set[`${table}.${name}`] = planned.keys[name];
      else if (!isNew) plan.remove.push(`${table}.${name}`);
    }
  }
  return null;
}

/** The form of the Models page, made from what the server told. */
export interface ModelsForm {
  upstreams: UpstreamForm[];
  originalUpstreams: UpstreamForm[];
  classes: ClassForm[];
  originalClasses: ClassForm[];
  teacher: OptionalForm;
  decisionModel: OptionalForm;
}

export function readForm(server: ServerSettings): ModelsForm {
  const list = setting(server, "router.upstreams");
  const upstreams = Array.isArray(list)
    ? list.map((value, place) => readUpstream(value, place))
    : [];
  const table = setting(server, "router.classes");
  const classes = isObject(table)
    ? Object.entries(table).map(([name, models]) => readClass(name, models))
    : [];
  const optional = (specs: Spec[], keyed: boolean): OptionalForm => {
    const key = keyed
      ? readKey(
          isSet(server, "decision_model.api_key_env")
            ? setting(server, "decision_model.api_key_env")
            : null,
          isSet(server, "decision_model.api_key_secret")
            ? setting(server, "decision_model.api_key_secret")
            : null,
        )
      : null;
    const wasOn = specs.some((spec) => isSet(server, spec.key));
    return {
      wasOn,
      on: wasOn,
      fields: specs.map((spec) => readField(spec, server)),
      key: key && { now: key, before: { ...key } },
    };
  };
  return {
    upstreams,
    originalUpstreams: structuredClone(upstreams),
    classes,
    originalClasses: structuredClone(classes),
    teacher: optional(TEACHER, false),
    decisionModel: optional(DECISION_MODEL, true),
  };
}

/** What the Models page changed: the keys to store, to set and to remove; or what is wrong with the inputs. */
export function modelsChanges(form: ModelsForm, secrets: string[]): Plan | string {
  const plan: Plan = { store: [], set: {}, remove: [] };
  const taken = new Set(secrets);
  for (const upstream of form.upstreams) if (upstream.key.stored) taken.add(upstream.key.stored);
  const fresh = (base: string) => {
    for (let n = 1; ; n += 1) {
      const name = base === "decision-model" && n === 1 ? base : `${base}-${n}`;
      if (!taken.has(name)) {
        taken.add(name);
        return name;
      }
    }
  };

  // Rows that are gone are removed from the last to the first, so that the places of the others hold; the rows that
  // changed, and the ones added, are set at the place they have after that.
  const kept = new Set(
    form.upstreams.flatMap((upstream) => (upstream.origin === null ? [] : [upstream.origin])),
  );
  for (let gone = form.originalUpstreams.length - 1; gone >= 0; gone -= 1) {
    if (!kept.has(gone)) plan.remove.push(`router.upstreams.${gone}`);
  }
  for (const [place, upstream] of form.upstreams.entries()) {
    const original = upstream.origin === null ? undefined : form.originalUpstreams[upstream.origin];
    if (original !== undefined && same(original, upstream)) continue;
    if (upstream.baseUrl.trim() === "") return "Enter the base URL of each provider.";
    const planned = planKey(upstream.key, `the provider ${place + 1}`, () => fresh("upstream"));
    if (typeof planned === "string") return planned;
    if (planned.store) plan.store.push(planned.store);
    const table: Obj = {
      protocol: upstream.protocol,
      base_url: upstream.baseUrl.trim(),
      ...planned.keys,
    };
    if (upstream.oauth !== null) table.oauth = upstream.oauth;
    plan.set[`router.upstreams.${place}`] = table;
  }

  if (!same(form.classes, form.originalClasses)) {
    // The classes are written whole, in place of what the file had.
    const classes: Obj = {};
    for (const each of form.classes) {
      const name = each.name.trim();
      if (name === "") return "Enter the name of each model class.";
      const entries = classEntries(each);
      if (entries.length === 0) return `Enter the models of the class ${name}.`;
      if (name in classes) return `There are two classes named ${name}.`;
      classes[name] = entries;
    }
    plan.remove.push("router.classes");
    if (Object.keys(classes).length > 0) plan.set["router.classes"] = classes;
  }

  const teacher = optionalChanges(form.teacher, "teacher", plan, () => "");
  if (teacher) return teacher;
  const decision = optionalChanges(form.decisionModel, "decision_model", plan, () =>
    fresh("decision-model"),
  );
  if (decision) return decision;
  return plan;
}

/** Whether the form holds changes that are not saved. */
export function modelsDirty(form: ModelsForm): boolean {
  const optionalDirty = (optional: OptionalForm) =>
    optional.wasOn !== optional.on ||
    optional.fields.some(changed) ||
    (optional.key !== null && !same(optional.key.now, optional.key.before));
  return (
    !same(form.upstreams, form.originalUpstreams) ||
    !same(form.classes, form.originalClasses) ||
    optionalDirty(form.teacher) ||
    optionalDirty(form.decisionModel)
  );
}

// --- logging in to a provider ------------------------------------------------------------------------------------

/** The fields of an `oauth` table typed for "Other". */
export interface OtherForm {
  protocol: string;
  baseUrl: string;
  flow: string;
  clientId: string;
  url: string;
  tokenUrl: string;
  scopes: string;
}

export const blankOther = (): OtherForm => ({
  protocol: PROTOCOLS[0],
  baseUrl: "",
  flow: FLOWS[0],
  clientId: "",
  url: "",
  tokenUrl: "",
  scopes: "",
});

/** The upstream of an `oauth` table typed for "Other", or what is missing. `secretSuffix` makes the name of its login's secret. */
export function otherUpstream(other: OtherForm, secretSuffix: number): Obj | string {
  const needed: Array<[string, string]> = [
    [other.baseUrl, "Base URL"],
    [other.clientId, "Client ID"],
    [other.url, other.flow === "device" ? "Device URL" : "Authorize URL"],
    [other.tokenUrl, "Token URL"],
  ];
  const missing = needed.find(([value]) => value.trim() === "");
  if (missing) return `${missing[1]} is empty.`;
  const oauth: Obj = {
    flow: other.flow,
    client_id: other.clientId.trim(),
    token_url: other.tokenUrl.trim(),
    secret: `login-${secretSuffix}`,
  };
  oauth[other.flow === "device" ? "device_url" : "authorize_url"] = other.url.trim();
  const scopes = other.scopes.split(/\s+/).filter((scope) => scope !== "");
  if (scopes.length > 0) oauth.scopes = scopes;
  return { protocol: other.protocol, base_url: other.baseUrl.trim(), oauth };
}

/** The upstream a built-in provider adds. */
export function presetUpstream(provider: {
  name: string;
  protocol: string;
  base_url: string;
}): Obj {
  return {
    protocol: provider.protocol,
    base_url: provider.base_url,
    oauth: { preset: provider.name },
  };
}
