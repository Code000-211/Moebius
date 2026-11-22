// @vitest-environment jsdom
/**
 * The editor's ground is the dark code ground in both themes, so the gutter, the active line and the selection are
 * drawn from the code tokens. A light-theme fill (`--hover`, `--surface-sunken`) on it leaves line 2 unreadable.
 */
import { cleanup, render } from "@testing-library/react";
import { afterEach, expect, it } from "vitest";
import CodeEditor from "./CodeEditor";

afterEach(cleanup);

it("draws its gutter, active line and selection from the code tokens, not the page's fills", () => {
  render(<CodeEditor value={"-- a\nb()"} onChange={() => {}} label="Script" />);
  const css = Array.from(document.querySelectorAll("style"))
    .map((style) => style.textContent ?? "")
    .join("\n");
  expect(css).toContain("var(--code-text)");
  expect(css).toContain("var(--code-head)");
  expect(css).not.toContain("var(--surface-sunken)");
  expect(css).not.toContain("var(--hover)");
});
