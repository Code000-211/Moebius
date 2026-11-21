import { describe, expect, it } from "vitest";
import { inputPath, inputSummary, stepWords, type ToolCall, toolInput } from "./index";

describe("toolInput", () => {
  it("reads Pi's edit with several replacements and its older single form", () => {
    expect(
      toolInput("edit", {
        path: "a",
        edits: [
          { oldText: "x", newText: "y" },
          { oldText: "p", newText: "q" },
        ],
      }),
    ).toEqual({
      type: "edit",
      path: "a",
      changes: [
        { oldText: "x", newText: "y" },
        { oldText: "p", newText: "q" },
      ],
    });
    expect(toolInput("edit", { path: "a", oldText: "x", newText: "y" })).toMatchObject({
      changes: [{ oldText: "x", newText: "y" }],
    });
  });

  it("reads Claude Code's tools, whose names are capitalised", () => {
    expect(toolInput("Edit", { file_path: "a", old_string: "x", new_string: "y" })).toEqual({
      type: "edit",
      path: "a",
      changes: [{ oldText: "x", newText: "y" }],
    });
    expect(toolInput("Bash", { command: "ls\nwc" })).toEqual({
      type: "command",
      command: "ls\nwc",
    });
    expect(
      toolInput("TodoWrite", { todos: [{ content: "Write tests", status: "in_progress" }] }),
    ).toEqual({ type: "todos", todos: [{ content: "Write tests", status: "in_progress" }] });
  });

  it("summarises an input in one line", () => {
    expect(inputSummary(toolInput("bash", { command: "ls\nwc" }))).toBe("ls");
    // A file is named by its name: the directory it sits in is in the tooltip, not in the row.
    expect(inputSummary(toolInput("read", { path: "/srv/app/docs/a.md" }))).toBe("a.md");
    expect(inputPath(toolInput("read", { path: "/srv/app/docs/a.md" }))).toBe("/srv/app/docs/a.md");
    expect(inputPath(toolInput("bash", { command: "ls" }))).toBeNull();
    expect(inputSummary(toolInput("grep", { pattern: "x" }))).toBe('{"pattern":"x"}');
    expect(inputSummary(toolInput("grep", null))).toBe("");
  });
});

describe("stepWords", () => {
  const call = (tool: string, args: unknown, running = false): ToolCall => ({
    id: "c",
    tool,
    input: toolInput(tool, args),
    check: null,
    mcp: null,
    execution: running ? { state: "running" } : { state: "finished", failed: false },
    output: "",
  });

  it("says a file call with its file apart, so the file can be set in mono", () => {
    expect(stepWords(call("read", { path: "/srv/repo/src/wiki.rs" }))).toEqual({
      icon: "read",
      text: "Read",
      code: "wiki.rs",
    });
    expect(stepWords(call("edit", { path: "/a/b.rs", edits: [] }))).toMatchObject({
      text: "Edited",
      code: "b.rs",
    });
  });

  it("says a command in a few plain words, never as the command", () => {
    const said = (command: string) => stepWords(call("bash", { command })).text;
    expect(said("cargo test -p cart discount")).toBe("Ran the tests");
    expect(said("cd web && pnpm test")).toBe("Ran the tests");
    expect(said('rg -n "fn export" src')).toBe("Searched the code");
    expect(said("cargo clippy --workspace -- -D warnings")).toBe("Ran the checks");
    expect(said("git diff main...HEAD --stat")).toBe("Looked at the changes");
    expect(said('"$MOEBIUS" board 1')).toBe("Looked at the board");
    expect(said("./frobnicate --now")).toBe("Ran a command");
  });

  it("says what is still running in the present tense", () => {
    expect(stepWords(call("bash", { command: "cargo test" }, true)).text).toBe("Running the tests");
    expect(stepWords(call("read", { path: "a.rs" }, true)).text).toBe("Reading");
  });
});
