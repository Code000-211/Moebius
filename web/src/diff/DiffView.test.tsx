// @vitest-environment jsdom
/**
 * A diff read in two columns: the left one holds the paths of the diff nested into the folders they sit in — the same
 * rows the repository's own tree wears — and choosing one moves the lines in the right one. What is worth testing is
 * the nesting, the folder that closes, and which file the diff opens with, because none of that is in the diff text.
 */
import { cleanup, fireEvent, render, screen } from "@testing-library/react";
import { afterEach, describe, expect, it } from "vitest";
import { DiffView } from "./DiffView";

afterEach(cleanup);

const DIFF = `diff --git a/src/lib.rs b/src/lib.rs
--- a/src/lib.rs
+++ b/src/lib.rs
@@ -1,1 +1,1 @@
-old line
+new line
diff --git a/notes.txt b/notes.txt
--- a/notes.txt
+++ b/notes.txt
@@ -1,1 +1,2 @@
 keep
+a note for the reviewer
`;

/** The left column as it reads from the top down, without the cost each file carries. */
const names = () =>
  screen.getAllByTestId("diff-file").map((row) => (row.textContent ?? "").replace(/\+\d+.*$/, ""));

describe("a diff read in two columns", () => {
  it("nests the paths into folders instead of writing them out again", () => {
    render(<DiffView diff={DIFF} />);
    const list = screen.getByRole("list", { name: "Changed files" });
    expect(list.textContent).toContain("src");
    expect(names()).toEqual(["lib.rs", "notes.txt"]);
  });

  it("carries what each file costs on its row", () => {
    render(<DiffView diff={DIFF} />);
    expect(screen.getAllByTestId("diff-file")[0]?.textContent).toContain("+1");
  });

  it("opens with the first file the diff names", () => {
    render(<DiffView diff={DIFF} />);
    expect(screen.getByTestId("diff-path").textContent).toBe("src/lib.rs");
  });

  it("moves the lines when a name is chosen, and marks the row it moved for", () => {
    render(<DiffView diff={DIFF} />);
    const note = screen.getByRole("button", { name: /notes\.txt/ });
    fireEvent.click(note);
    expect(screen.getByTestId("diff-path").textContent).toBe("notes.txt");
    expect(note.getAttribute("aria-current")).toBe("true");
    expect(screen.getByRole("button", { name: /lib\.rs/ }).getAttribute("aria-current")).toBeNull();
    expect(screen.getByText("a note for the reviewer")).toBeTruthy();
  });

  it("closes a folder and opens it again", () => {
    render(<DiffView diff={DIFF} />);
    const folder = screen.getByRole("button", { name: "src" });
    expect(folder.getAttribute("aria-expanded")).toBe("true");
    fireEvent.click(folder);
    expect(screen.getAllByTestId("diff-file").map((row) => row.textContent)).toEqual([
      "notes.txt+1 added -0 removed",
    ]);
    fireEvent.click(screen.getByRole("button", { name: "src" }));
    expect(names()).toEqual(["lib.rs", "notes.txt"]);
  });

  it("keeps the file's header and lines on the dark colour of code, with the colours of code on it", () => {
    render(<DiffView diff={DIFF} />);
    expect(screen.getByTestId("diff-lines").className).toContain("bg-code-bg");
    expect(screen.getByTestId("diff-path").className).toContain("text-code-text");
    const header = screen.getByTestId("diff-path").parentElement as HTMLElement;
    expect(header.className).toContain("bg-code-bg");
    expect(header.innerHTML).not.toMatch(
      /text-text|text-secondary|text-running-text|text-error-text/,
    );
    const body = screen.getByTestId("diff-lines");
    expect(body.innerHTML).not.toMatch(
      /text-text|text-secondary|text-complete-text|text-error-text/,
    );
    expect(body.querySelector("[data-kind='added'] .text-code-added")).not.toBeNull();
    expect(body.querySelector("[data-kind='removed'] .text-code-error")).not.toBeNull();
  });
});
