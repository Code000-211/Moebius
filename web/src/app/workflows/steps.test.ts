import { describe, expect, it } from "vitest";
import type { WorkflowDefinition, WorkflowNode } from "../../api/types/workflows";
import {
  kindTitle,
  outputName,
  pieces,
  plainText,
  stepSummary,
  stepsInOrder,
  truncate,
} from "./steps";

const at = { x: 0, y: 0 };
const node = (kind: string, params: Record<string, unknown>, id = "n"): WorkflowNode => ({
  id,
  kind,
  params,
  position: at,
});
const text = (item: WorkflowNode) => plainText(stepSummary(item));

describe("pieces", () => {
  it("sets expressions apart from the words around them", () => {
    expect(pieces("Summarise {{ input.summary }} for {{ trigger.tag }}.")).toEqual([
      { text: "Summarise " },
      { expr: "{{ input.summary }}" },
      { text: " for " },
      { expr: "{{ trigger.tag }}" },
      { text: "." },
    ]);
  });

  it("folds line breaks into spaces, so no escaped newline survives", () => {
    const folded = plainText(pieces("First line.\n\nSecond line,\r\n  indented."));
    expect(folded).toBe("First line. Second line, indented.");
    expect(folded).not.toContain("\\n");
    expect(folded).not.toContain("\n");
  });

  it("leaves an unclosed brace as words", () => {
    expect(pieces("a {{ b")).toEqual([{ text: "a {{ b" }]);
  });
});

describe("truncate", () => {
  it("cuts a long text at a word and ends it in an ellipsis", () => {
    const long = Array.from({ length: 60 }, (_, i) => `word${i}`).join(" ");
    const cut = plainText(truncate(pieces(long), 50));
    expect(cut.endsWith("…")).toBe(true);
    expect(cut.length).toBeLessThanOrEqual(51);
    expect(cut).toMatch(/word\d+…$/);
  });

  it("never cuts an expression in half", () => {
    const cut = truncate(
      pieces("Read the issue then {{ trigger.some.long.expression }} and go"),
      30,
    );
    expect(plainText(cut)).not.toContain("{{ trigger.some");
    expect(plainText(cut).endsWith("…")).toBe(true);
  });

  it("keeps a short text as it is", () => {
    expect(plainText(truncate(pieces("Short."), 50))).toBe("Short.");
  });
});

describe("stepSummary, per kind", () => {
  it("says an agent's prompt as plain text, however many lines it has", () => {
    const prompt = `Read the issue.\n\nThen do these:\n- write the fix\n- add a test\n${"More detail. ".repeat(40)}`;
    const summary = stepSummary(node("agent", { agent: "coder", prompt, output: "summary" }));
    const said = plainText(summary);
    expect(said.startsWith("Read the issue. Then do these: - write the fix")).toBe(true);
    expect(said.endsWith("…")).toBe(true);
    expect(said.length).toBeLessThanOrEqual(181);
    expect(said).not.toMatch(/\\n|\n/);
  });

  it("sets an expression of a prompt apart", () => {
    const summary = stepSummary(node("agent", { prompt: "Summarise {{ input.summary }}." }));
    expect(summary).toContainEqual({ expr: "{{ input.summary }}" });
  });

  it("says a publish by its title, and its body after a dash", () => {
    expect(text(node("publish", { title: "Release {{ trigger.tag }}" }))).toBe(
      "Release {{ trigger.tag }}",
    );
    expect(text(node("publish", { title: "Notes", body: "All\nchanges" }))).toBe(
      "Notes — All changes",
    );
  });

  it("says an http call by its method and URL, GET when there is no method", () => {
    expect(text(node("http", { url: "https://example.test/a", method: "post" }))).toBe(
      "POST https://example.test/a",
    );
    expect(text(node("http", { url: "https://example.test/{{ input.id }}" }))).toBe(
      "GET https://example.test/{{ input.id }}",
    );
  });

  it("says a condition by its ports and expressions, with its fallback", () => {
    const summary = stepSummary(
      node("condition", {
        ports: [
          { name: "big", when: "input.size > 10" },
          { name: "small", when: "input.size <= 10" },
        ],
        fallback: "none",
      }),
    );
    expect(plainText(summary)).toBe(
      "big: input.size > 10 · small: input.size <= 10 · otherwise none",
    );
    expect(summary).toContainEqual({ expr: "input.size > 10" });
  });

  it("says an approval by what it asks", () => {
    expect(
      text(node("approval", { prompt: "Publish 1.4.0\nto the registry?", timeout_seconds: 60 })),
    ).toBe("Publish 1.4.0 to the registry?");
  });

  it("says a code node by its first line of Luau", () => {
    const summary = stepSummary(node("code", { source: "\n  local x = input.n\nreturn x" }));
    expect(summary).toEqual([{ text: "Luau: " }, { expr: "local x = input.n" }]);
  });

  it("says a merge and a for_each in words", () => {
    expect(text(node("merge", { a: "left", b: "right" }))).toBe("Joins left and right");
    expect(text(node("for_each", { rejoin: "collect", max_elements: 5 }))).toBe(
      "For every element, then rejoins at collect, at most 5",
    );
  });

  it("says a kind it does not know by its parameters in words, without JSON", () => {
    const said = text(
      node("mystery", { size: 3, tags: ["a", "b"], deep: { x: 1, y: 2 }, note: "hi\nthere" }),
    );
    expect(said).toBe("size: 3 · tags: a, b · deep: 2 fields · note: hi there");
    expect(said).not.toMatch(/[{}[\]"]/);
  });

  it("says so when a node takes nothing", () => {
    expect(text(node("agent", {}))).toBe("Takes no parameters.");
  });
});

describe("kindTitle and outputName", () => {
  it("names the agent of an agent node", () => {
    expect(kindTitle(node("agent", { agent: "Coder" }))).toBe("Agent · Coder");
    expect(kindTitle(node("agent", {}))).toBe("Agent");
    expect(kindTitle(node("http", {}))).toBe("HTTP");
    expect(kindTitle(node("mystery", {}))).toBe("mystery");
  });

  it("gives the output name, when there is one", () => {
    expect(outputName(node("agent", { output: "summary" }))).toBe("summary");
    expect(outputName(node("agent", { output: " " }))).toBeNull();
    expect(outputName(node("http", {}))).toBeNull();
  });
});

describe("stepsInOrder", () => {
  const graph = (nodes: string[], edges: Array<[string, string]>): WorkflowDefinition => ({
    name: "w",
    description: "",
    trigger: [{ kind: "manual" }],
    nodes: nodes.map((id) => node("code", { source: "return input" }, id)),
    edges: edges.map(([from, to]) => ({ from, to, port: "out" })),
  });
  const ids = (definition: WorkflowDefinition) => stepsInOrder(definition).map((each) => each.id);

  it("follows the edges rather than the order of the file", () => {
    expect(
      ids(
        graph(
          ["c", "b", "a"],
          [
            ["trigger", "a"],
            ["a", "b"],
            ["b", "c"],
          ],
        ),
      ),
    ).toEqual(["a", "b", "c"]);
  });

  it("puts a node after every node that leads to it", () => {
    const order = ids(
      graph(
        ["join", "x", "y"],
        [
          ["trigger", "x"],
          ["trigger", "y"],
          ["x", "join"],
          ["y", "join"],
        ],
      ),
    );
    expect(order).toEqual(["x", "y", "join"]);
  });

  it("ends with a node nothing leads to", () => {
    expect(ids(graph(["lost", "a"], [["trigger", "a"]]))).toEqual(["a", "lost"]);
  });

  it("does not loop on a cycle", () => {
    expect(
      ids(
        graph(
          ["a", "b"],
          [
            ["trigger", "a"],
            ["a", "b"],
            ["b", "a"],
          ],
        ),
      ).sort(),
    ).toEqual(["a", "b"]);
  });
});
