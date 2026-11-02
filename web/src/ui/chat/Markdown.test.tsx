// @vitest-environment jsdom
/**
 * The headings an agent writes with `#` are part of a page that already has a title of its own, so they are drawn one
 * level under the Markdown that asks for them. A page's outline stays one `<h1>` deep and then the agent's sections.
 * The other half of what an agent writes with is the newline: in a chat it is content and is drawn, in a Markdown file
 * it is the author's own wrapping and is not.
 */
import { cleanup, render } from "@testing-library/react";
import { afterEach, describe, expect, it } from "vitest";
import { Markdown } from "./Markdown";

afterEach(cleanup);

/** Every heading the render drew, as its tag and its words. */
function headings(): Array<[string, string]> {
  return [...document.querySelectorAll("h1, h2, h3, h4, h5, h6")].map((node) => [
    node.tagName.toLowerCase(),
    node.textContent ?? "",
  ]);
}

describe("the headings of an answer", () => {
  it("start below the page's own title", () => {
    render(<Markdown content={"# One\n\n## Two\n\n### Three\n\n#### Four"} />);
    expect(headings()).toEqual([
      ["h2", "One"],
      ["h3", "Two"],
      ["h4", "Three"],
      ["h5", "Four"],
    ]);
  });

  it("keeps the size and the weight each level was drawn at", () => {
    const { container } = render(<Markdown content={"# One\n\n#### Four"} />);
    const drawn = (tag: string) => container.querySelector(tag)?.getAttribute("class") ?? "";
    expect(drawn("h2")).toContain("text-xl");
    expect(drawn("h2")).toContain("font-semibold");
    expect(drawn("h5")).toContain("text-base");
    expect(drawn("h5")).toContain("font-semibold");
  });
});

describe("the newlines of an answer", () => {
  /** The paragraph's own classes, and those it is given from its wrapper. */
  function drawn(keepNewlines?: boolean) {
    const { container } = render(
      <Markdown content={"first line\nsecond line"} keepNewlines={keepNewlines} />,
    );
    return container.querySelector("div")?.getAttribute("class") ?? "";
  }

  it("draws an agent's line as a line", () => {
    expect(drawn(true)).toContain("[&_p]:whitespace-pre-wrap");
  });

  it("leaves a Markdown file's wrapping to the file", () => {
    expect(drawn()).not.toContain("whitespace-pre-wrap");
  });
});
