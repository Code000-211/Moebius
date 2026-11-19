/**
 * The logic of the pages about kinds of issues, apart from React: a kind as the form of the Kinds page holds it and the
 * body it sends, which kinds a project offers, the fields every kind has between them, and what the Issues page offers
 * its filter bar and sends the server.
 */
import type { Card } from "../../api/types/board";
import type { FieldType, Kind, KindBody } from "../../api/types/kinds";
import type { Project } from "../../api/types/projects";
import type { Person } from "../../api/types/server";
import type { DisplayChoices, FilterField } from "../../ui/filters/model";

/** A field as the form holds it: the options of a choice are one line of text, separated by commas. */
export interface FieldDraft {
  name: string;
  type: FieldType;
  options: string;
}

/** A kind as the form holds it. */
export interface KindDraft {
  name: string;
  fields: FieldDraft[];
}

/** The form of `kind`, or an empty one for a new kind. */
export function draftOf(kind?: Kind): KindDraft {
  return {
    name: kind?.name ?? "",
    fields: (kind?.fields ?? []).map((f) => ({
      name: f.name,
      type: f.type,
      options: f.options.join(", "),
    })),
  };
}

/** What the form sends: names trimmed, the options of a choice split at commas, any other type with none. */
export function bodyOf(draft: KindDraft): KindBody {
  return {
    name: draft.name.trim(),
    fields: draft.fields.map((f) => ({
      name: f.name.trim(),
      type: f.type,
      options:
        f.type === "choice"
          ? f.options
              .split(",")
              .map((o) => o.trim())
              .filter((o) => o !== "")
          : [],
    })),
  };
}

/** The kinds a project offers, in the order the kinds were made. */
export function kindsOf(kinds: readonly Kind[], project: Pick<Project, "id">): Kind[] {
  return kinds.filter((kind) => kind.projects.includes(project.id));
}

/** The names of the fields the kinds have between them, each once whatever its case, in the order met. */
export function fieldNames(kinds: readonly Kind[]): string[] {
  const seen = new Map<string, string>();
  for (const kind of kinds) {
    for (const field of kind.fields) {
      if (!seen.has(field.name.toLowerCase())) seen.set(field.name.toLowerCase(), field.name);
    }
  }
  return [...seen.values()];
}

/** The URL key and the server's parameter of the filter by the field `name`. */
export function fieldKey(name: string): string {
  return `field.${name}`;
}

/** The sort the address names: `asc:Estimate` or `desc:Estimate`, or none for the order the cards were made in. */
export function sortOf(value: string): { field: string; descending: boolean } | null {
  const match = /^(asc|desc):(.+)$/.exec(value);
  return match ? { field: match[2] ?? "", descending: match[1] === "desc" } : null;
}

/** The filters the Issues page offers: project, kind, state and one for every field, across kinds. */
export function issueFilterFields(
  kinds: readonly Kind[],
  projects: readonly Project[],
  cards: readonly Card[],
  people: readonly Person[],
  values: Readonly<Record<string, string>>,
): FilterField[] {
  const fields: FilterField[] = [
    { key: "project", label: "Project", options: projects.map((p) => [String(p.id), p.name]) },
    { key: "kind", label: "Kind", options: kinds.map((k) => [k.name, k.name]) },
    {
      key: "state",
      label: "State",
      options: [
        ["open", "Open"],
        ["closed", "Closed"],
      ],
    },
  ];
  for (const name of fieldNames(kinds)) {
    const key = fieldKey(name);
    const same = kinds
      .flatMap((k) => k.fields)
      .filter((f) => f.name.toLowerCase() === name.toLowerCase());
    const found = new Set<string>();
    for (const field of same) {
      if (field.type === "choice") for (const option of field.options) found.add(option);
      if (field.type === "person") {
        for (const p of people) if (!p.disabled) found.add(p.handle);
      }
    }
    // A value another type holds is offered from the cards that have it, so a filter can be picked from what is there.
    for (const card of cards) {
      const value = Object.entries(card.fields ?? {}).find(
        ([n]) => n.toLowerCase() === name.toLowerCase(),
      )?.[1];
      if (value !== undefined) found.add(String(value));
    }
    if (values[key] && !found.has(values[key])) found.add(values[key]);
    fields.push({
      key,
      label: name,
      options: [...found]
        .sort((a, b) => a.localeCompare(b, undefined, { numeric: true }))
        .map((v) => [v, v]),
    });
  }
  return fields;
}

/** What Display offers on the Issues page: no layouts or fields to show, only the order. */
export function issueDisplayChoices(kinds: readonly Kind[]): DisplayChoices {
  const sorts: Array<[string, string]> = [["", "Oldest first"]];
  for (const name of fieldNames(kinds)) {
    sorts.push(
      [`asc:${name}`, `${name}, lowest first`],
      [`desc:${name}`, `${name}, highest first`],
    );
  }
  return { groups: [], sorts, fields: [], layouts: [] };
}

/** The parameters of `GET /v1/issues` for what the address holds. */
export function issueQuery(params: URLSearchParams): URLSearchParams {
  const query = new URLSearchParams();
  for (const [key, value] of params) {
    if (value === "") continue;
    if (key === "project" || key === "kind" || key === "state" || key.startsWith("field.")) {
      query.append(key, value);
    }
  }
  const sort = sortOf(params.get("sort") ?? "");
  if (sort) {
    query.set("sort", sort.field);
    if (sort.descending) query.set("order", "desc");
  }
  return query;
}

/** The fields of a card that are set, as "Severity high · Estimate 3". */
export function fieldsLine(card: Pick<Card, "fields">): string {
  return Object.entries(card.fields ?? {})
    .map(([name, value]) => `${name} ${value}`)
    .join(" · ");
}
