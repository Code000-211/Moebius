/**
 * The steps of a workflow in plain words: the nodes in graph order, and for each one a line a person can read — its
 * kind, a short summary of what it is given and the name of what it gives back. The Overview tab lists them and the
 * canvas cards use the same summary, so the two never describe a node differently.
 *
 * A summary is a list of pieces, text and expressions, so a page can set an expression such as `{{ input.summary }}`
 * in the code font and the rest as prose. Nothing here prints JSON: a prompt of several lines is one line of words
 * with its line breaks folded into spaces, cut at a word with "…" when it is long, and a parameter that is a list or
 * an object is said by what it holds, never by its brackets.
 */
import type { WorkflowDefinition, WorkflowNode } from "../../api/types/workflows";
import { nodeKinds } from "./nodeKinds";

/** One piece of a summary: words, or an expression that is set in the code font. */
export type Piece = { text: string } | { expr: string };

/** How many characters of a long parameter a summary keeps before it ends in "…". */
export const SUMMARY_LENGTH = 180;

/** The id the definition gives the start of the graph in its edges. */
const TRIGGER = "trigger";

/** The words and `{{ expressions }}` of `text`, the line breaks folded into single spaces. */
export function pieces(text: string): Piece[] {
  const folded = text.replace(/\s+/g, " ").trim();
  const out: Piece[] = [];
  let rest = folded;
  for (;;) {
    const open = rest.indexOf("{{");
    const close = open < 0 ? -1 : rest.indexOf("}}", open + 2);
    if (open < 0 || close < 0) {
      if (rest !== "") out.push({ text: rest });
      return out;
    }
    if (open > 0) out.push({ text: rest.slice(0, open) });
    out.push({ expr: rest.slice(open, close + 2) });
    rest = rest.slice(close + 2);
  }
}

/** The text of a list of pieces, as it reads. */
export function plainText(list: readonly Piece[]): string {
  return list.map((piece) => ("text" in piece ? piece.text : piece.expr)).join("");
}

/**
 * `list` cut to `limit` characters: at the last space of the text that is cut, never inside an expression, and with
 * "…" after it. A list that fits is returned as it is.
 */
export function truncate(list: readonly Piece[], limit: number = SUMMARY_LENGTH): Piece[] {
  if (plainText(list).length <= limit) return [...list];
  const out: Piece[] = [];
  let used = 0;
  for (const piece of list) {
    if ("expr" in piece) {
      if (used + piece.expr.length > limit) break;
      out.push(piece);
      used += piece.expr.length;
      continue;
    }
    const room = limit - used;
    if (piece.text.length <= room) {
      out.push(piece);
      used += piece.text.length;
      continue;
    }
    const cut = piece.text.slice(0, room);
    const space = cut.lastIndexOf(" ");
    const kept = (space > 0 ? cut.slice(0, space) : cut).trimEnd();
    if (kept !== "") out.push({ text: kept });
    break;
  }
  const last = out[out.length - 1];
  if (last && "text" in last) {
    out[out.length - 1] = { text: `${last.text.trimEnd()}…` };
  } else {
    out.push({ text: "…" });
  }
  return out;
}

function str(value: unknown): string {
  return typeof value === "string" ? value : "";
}

/** A parameter said in words: a string as it is, a number or flag plainly, a list by its items, an object by its size. */
function said(value: unknown): string {
  if (typeof value === "string") return value;
  if (typeof value === "number" || typeof value === "boolean") return String(value);
  if (Array.isArray(value)) {
    return value.every((each) => typeof each === "string" || typeof each === "number")
      ? value.join(", ")
      : `${value.length} ${value.length === 1 ? "item" : "items"}`;
  }
  if (value !== null && typeof value === "object") {
    const count = Object.keys(value).length;
    return `${count} ${count === 1 ? "field" : "fields"}`;
  }
  return "";
}

/** What kind of step a node is, as a title: `Agent · Coder` for an agent node that names its agent. */
export function kindTitle(node: WorkflowNode): string {
  const known = nodeKinds.find((each) => each.kind === node.kind);
  const title = known?.title ?? node.kind;
  const agent = node.kind === "agent" ? str(node.params?.agent) : "";
  return agent === "" ? title : `${title} · ${agent}`;
}

/** The name of what a node gives back, when its parameters name one. */
export function outputName(node: WorkflowNode): string | null {
  const name = str(node.params?.output).trim();
  return name === "" ? null : name;
}

/** One line of Luau: the first that says something, so a person knows which script it is. */
function firstLine(source: string): string {
  return (
    source
      .split("\n")
      .map((line) => line.trim())
      .find((line) => line !== "") ?? ""
  );
}

/** What a node is given, in words, cut where it is long. A node with nothing to say says so. */
export function stepSummary(node: WorkflowNode): Piece[] {
  const params = (node.params ?? {}) as Record<string, unknown>;
  const list = summaryPieces(node.kind, params);
  return truncate(list.length > 0 ? list : [{ text: "Takes no parameters." }]);
}

function summaryPieces(kind: string, params: Record<string, unknown>): Piece[] {
  switch (kind) {
    case "agent":
    case "approval":
    case "chat":
      return pieces(str(params.prompt));
    case "publish": {
      const title = str(params.title);
      const body = str(params.body);
      return body === "" ? pieces(title) : [...pieces(title), { text: " — " }, ...pieces(body)];
    }
    case "http": {
      const method = str(params.method).trim() === "" ? "GET" : str(params.method).toUpperCase();
      return [{ text: `${method} ` }, ...pieces(str(params.url))];
    }
    case "condition": {
      const ports = Array.isArray(params.ports) ? params.ports : [];
      const out: Piece[] = [];
      for (const entry of ports) {
        const port = (entry ?? {}) as Record<string, unknown>;
        if (out.length > 0) out.push({ text: " · " });
        out.push({ text: `${str(port.name)}: ` }, { expr: str(port.when) });
      }
      const fallback = str(params.fallback);
      if (fallback !== "")
        out.push({ text: `${out.length > 0 ? " · " : ""}otherwise ${fallback}` });
      return out;
    }
    case "code": {
      const line = firstLine(str(params.source));
      return line === "" ? [] : [{ text: "Luau: " }, { expr: line }];
    }
    case "merge":
      return [{ text: `Joins ${str(params.a)} and ${str(params.b)}` }];
    case "for_each": {
      const most =
        typeof params.max_elements === "number" ? `, at most ${params.max_elements}` : "";
      return [{ text: `For every element, then rejoins at ${str(params.rejoin)}${most}` }];
    }
    default:
      return paramsInWords(params);
  }
}

/** Parameters as `name: value · name: value`, each value in words; what has no value is left out. */
export function paramsInWords(params: Record<string, unknown> | undefined): Piece[] {
  const out: Piece[] = [];
  for (const [name, value] of Object.entries(params ?? {})) {
    const text = said(value);
    if (text === "") continue;
    out.push({ text: `${out.length > 0 ? " · " : ""}${name}: ` }, ...pieces(text));
  }
  return out;
}

/**
 * The nodes in the order the graph runs them: those the start leads into first, then each node after the nodes that
 * lead to it, ties in the order of the file. A node nothing leads to, and the rest of a cycle the server would refuse,
 * follow in the order of the file.
 */
export function stepsInOrder(definition: WorkflowDefinition): WorkflowNode[] {
  const byId = new Map(definition.nodes.map((node) => [node.id, node]));
  const waiting = new Map(definition.nodes.map((node) => [node.id, 0]));
  for (const edge of definition.edges) {
    if (edge.from !== TRIGGER && byId.has(edge.from) && byId.has(edge.to)) {
      waiting.set(edge.to, (waiting.get(edge.to) ?? 0) + 1);
    }
  }
  const first = new Set(definition.edges.filter((e) => e.from === TRIGGER).map((e) => e.to));
  const rank = (node: WorkflowNode) =>
    (first.has(node.id) ? 0 : 1) * definition.nodes.length + definition.nodes.indexOf(node);
  const ready = definition.nodes.filter((node) => waiting.get(node.id) === 0);
  const out: WorkflowNode[] = [];
  const done = new Set<string>();
  while (ready.length > 0) {
    ready.sort((a, b) => rank(a) - rank(b));
    const node = ready.shift() as WorkflowNode;
    out.push(node);
    done.add(node.id);
    for (const edge of definition.edges) {
      if (edge.from !== node.id) continue;
      const next = byId.get(edge.to);
      if (!next || done.has(next.id)) continue;
      const left = (waiting.get(next.id) ?? 1) - 1;
      waiting.set(next.id, left);
      if (left === 0) ready.push(next);
    }
  }
  return [...out, ...definition.nodes.filter((node) => !done.has(node.id))];
}
