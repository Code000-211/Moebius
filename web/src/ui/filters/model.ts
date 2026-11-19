/**
 * What the shared filter bar keeps and where: the filters are "field is value" pairs and the Display choices (layout,
 * grouping, sorting, the fields a card shows) sit beside them, all of it in the address so a reload or a link keeps
 * them. The functions here read and write that state as plain query parameters and know nothing of any one page; a
 * page names its fields and its choices and the bar and these functions do the rest.
 */

/** One thing a page can be filtered by: its key in the address, its name, and the values it offers as `[value, label]`. */
export interface FilterField {
  key: string;
  label: string;
  options: Array<[string, string]>;
}

/** The layouts a page can have: a board of cards and a list of them, and the timeline of milestones. */
export type Layout = "board" | "list" | "timeline";

/** What Display holds. An empty `group` or `sort` is the page's own default; `hidden` lists the fields switched off. */
export interface Display {
  layout: Layout;
  group: string;
  sort: string;
  hidden: string[];
}

export const DEFAULT_DISPLAY: Display = { layout: "board", group: "", sort: "", hidden: [] };

/** What a page lets Display choose from: the values it accepts for each choice (the rest are read as the default). */
export interface DisplayChoices {
  /** The layouts the page offers as `[value, label]`; a page of cards leaves it out and has Board and List, and a page with one layout gives none. */
  layouts?: Array<[string, string]>;
  groups: Array<[string, string]>;
  sorts: Array<[string, string]>;
  fields: Array<[string, string]>;
}

/** A filter that is set: the field, the value, and the words of each. */
export interface ActiveFilter {
  field: FilterField;
  value: string;
  words: string;
}

/** The filters that are set, in the order of the fields. A value the field no longer offers still shows, by its own text. */
export function activeFilters(
  fields: readonly FilterField[],
  values: Readonly<Record<string, string>>,
): ActiveFilter[] {
  const active: ActiveFilter[] = [];
  for (const field of fields) {
    const value = values[field.key] ?? "";
    if (value === "") continue;
    active.push({ field, value, words: field.options.find(([v]) => v === value)?.[1] ?? value });
  }
  return active;
}

/** The value of each field of the address, empty for a field that is not set. */
export function readFilters(
  params: URLSearchParams,
  fields: readonly FilterField[],
): Record<string, string> {
  return Object.fromEntries(fields.map((field) => [field.key, params.get(field.key) ?? ""]));
}

/** The layout the address names, when the page offers it; Board otherwise, which is no parameter at all. */
function layoutOf(value: string | null, choices: DisplayChoices): Layout {
  if (choices.layouts) {
    return choices.layouts.some(([each]) => each === value) ? (value as Layout) : "board";
  }
  return value === "list" ? "list" : "board";
}

/** Display as the address has it; a value the page does not offer is read as the default. */
export function readDisplay(params: URLSearchParams, choices: DisplayChoices): Display {
  const pick = (key: string, allowed: Array<[string, string]>) => {
    const value = params.get(key) ?? "";
    return allowed.some(([each]) => each === value) ? value : "";
  };
  const known = new Set(choices.fields.map(([key]) => key));
  const hidden = (params.get("hide") ?? "").split(",").filter((key) => known.has(key));
  return {
    layout: layoutOf(params.get("layout"), choices),
    group: pick("group", choices.groups),
    sort: pick("sort", choices.sorts),
    hidden: [...new Set(hidden)].sort(),
  };
}

/** Display as the parameters that carry it; a default is the empty string, which removes the parameter. */
export function displayQuery(display: Display): Record<string, string> {
  return {
    layout: display.layout === "board" ? "" : display.layout,
    group: display.group,
    sort: display.sort,
    hide: [...display.hidden].sort().join(","),
  };
}

/** Whether Display differs from its defaults. */
export function hasDisplay(display: Display): boolean {
  return Object.values(displayQuery(display)).some((value) => value !== "");
}

/** `hidden` with `key` switched on or off. */
export function toggled(hidden: readonly string[], key: string, shown: boolean): string[] {
  const rest = hidden.filter((each) => each !== key);
  return shown ? rest : [...rest, key].sort();
}

/** A view saved from the bar: a name, and the parameters of the address it holds. */
export interface SavedView {
  name: string;
  query: string;
}

/** The views kept under `stored` (the text of a browser's storage), none for text that is not a list of views. */
export function parseViews(stored: string | null): SavedView[] {
  try {
    const list: unknown = JSON.parse(stored ?? "[]");
    if (!Array.isArray(list)) return [];
    return list.filter(
      (each): each is SavedView =>
        typeof each?.name === "string" && typeof each?.query === "string",
    );
  } catch {
    return [];
  }
}

/** The views with one more, named after how many there are: "View 1", "View 2". */
export function withView(views: readonly SavedView[], query: string): SavedView[] {
  return [...views, { name: `View ${views.length + 1}`, query }];
}

/** The parameters of `keys` that are set in `params`, as a query string, for a saved view. */
export function viewQuery(params: URLSearchParams, keys: readonly string[]): string {
  const kept = new URLSearchParams();
  for (const key of keys) {
    const value = params.get(key);
    if (value !== null && value !== "") kept.set(key, value);
  }
  return kept.toString();
}
