import { describe, expect, it } from "vitest";
import { inputPath, inputSummary, toolInput } from "./index";

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
