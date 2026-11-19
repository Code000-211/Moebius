/**
 * The keys of the focus: a command's single key runs it while the thing is in focus and the keys belong to the page,
 * and not while a field, a dialog or a modifier has them; the focus goes with the thing that registered it. And the
 * places seen lately, as the palette keeps them.
 */
import { cleanup, fireEvent, render } from "@testing-library/react";
import { afterEach, describe, expect, it, vi } from "vitest";
import { PaletteFocusFor, PaletteFocusProvider } from "./focus";
import { matches, noteRecent, readRecent } from "./model";

afterEach(() => {
  cleanup();
  window.localStorage.clear();
});

function page(run: () => void, shown = true) {
  const focus = { label: "#1 A", commands: [{ id: "assign", label: "Assign", key: "a", run }] };
  return (
    <PaletteFocusProvider>
      <input aria-label="field" />
      {shown && <PaletteFocusFor focus={focus} />}
    </PaletteFocusProvider>
  );
}

describe("the keys of the focus", () => {
  it("run the command whose key is typed on the page", () => {
    const run = vi.fn();
    render(page(run));
    fireEvent.keyDown(document.body, { key: "a" });
    fireEvent.keyDown(document.body, { key: "A" });
    fireEvent.keyDown(document.body, { key: "z" });
    expect(run).toHaveBeenCalledTimes(2);
  });

  it("leave a field, a modifier and an open dialog their keys", () => {
    const run = vi.fn();
    const { getByLabelText } = render(page(run));
    fireEvent.keyDown(getByLabelText("field"), { key: "a" });
    fireEvent.keyDown(document.body, { key: "a", ctrlKey: true });
    const dialog = document.createElement("div");
    dialog.setAttribute("role", "dialog");
    dialog.setAttribute("aria-modal", "true");
    dialog.setAttribute("data-state", "open");
    document.body.append(dialog);
    fireEvent.keyDown(document.body, { key: "a" });
    dialog.remove();
    expect(run).not.toHaveBeenCalled();
  });

  it("go when the thing in focus goes", () => {
    const run = vi.fn();
    const { rerender } = render(page(run));
    rerender(page(run, false));
    fireEvent.keyDown(document.body, { key: "a" });
    expect(run).not.toHaveBeenCalled();
  });
});

describe("matching and recent places", () => {
  it("matches every word, in any case, and everything for an empty query", () => {
    expect(matches("", "Anything")).toBe(true);
    expect(matches("fix LEX", "#12 Fix the lexer card")).toBe(true);
    expect(matches("fix parser", "#12 Fix the lexer card")).toBe(false);
  });

  it("keeps the latest place first, once, and a few of them", () => {
    for (const path of ["/a", "/b", "/c", "/a", "/d", "/e", "/f", "/g"]) noteRecent(path);
    const kept = readRecent();
    expect(kept[0]).toBe("/g");
    expect(kept).toHaveLength(6);
    expect(new Set(kept).size).toBe(6);
  });

  it("reads nothing from storage that is not a list", () => {
    window.localStorage.setItem("moebius.recent", "{");
    expect(readRecent()).toEqual([]);
    window.localStorage.setItem("moebius.recent", '{"a":1}');
    expect(readRecent()).toEqual([]);
  });
});
