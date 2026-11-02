// @vitest-environment jsdom
/**
 * A Copy button waits until the block it belongs to is being looked at. The words are the content of a message; the
 * button is only a way of carrying them off, and a row of them beside every message at once is furniture. The rule is
 * written once, as `copy-quiet` on the button and `reveal-copy` on the block in `styles/tailwind.css`, so this checks
 * both halves: that the button carries the name, and that the stylesheet actually hides it until the block is hovered
 * or focused — and still shows it where there is nothing to hover.
 */
import { readFileSync } from "node:fs";
import { cleanup, render, screen } from "@testing-library/react";
import { afterEach, describe, expect, it } from "vitest";
import { CopyButton } from "./CopyButton";

afterEach(cleanup);

// The rule that hides the button lives in the stylesheet, not in this component, so it is read from the file. In jsdom
// `import.meta.url` is an address on the page rather than a path, so it is taken from the package, which is where
// `pnpm test` runs.
const tailwind = readFileSync("src/styles/tailwind.css", "utf8");

/** The body of one `@utility` block, newlines kept. */
function utility(name: string): string {
  const start = tailwind.indexOf(`@utility ${name}`);
  expect(start, `tailwind.css defines @utility ${name}`).toBeGreaterThan(-1);
  const open = tailwind.indexOf("{", start);
  let depth = 0;
  for (let at = open; at < tailwind.length; at += 1) {
    if (tailwind[at] === "{") depth += 1;
    else if (tailwind[at] === "}") {
      if (depth === 1) return tailwind.slice(open, at + 1);
      depth -= 1;
    }
  }
  throw new Error(`@utility ${name} is not closed`);
}

describe("a Copy button at rest", () => {
  it("is carried by the class the block reveals", () => {
    render(<CopyButton text="the message" label="Copy message" />);
    expect(screen.getByRole("button", { name: "Copy message" }).getAttribute("class")).toContain(
      "copy-quiet",
    );
  });

  it("is out of the way until its block is looked at", () => {
    const quiet = utility("copy-quiet");
    expect(quiet).toContain("opacity: 0");
    // A keyboard reaches what a pointer does, and a screen with no pointer has nothing to hover.
    expect(quiet).toContain("&:focus-visible");
    expect(quiet).toContain("@media (hover: none)");
    expect(utility("reveal-copy")).toContain("&:hover .copy-quiet");
    expect(utility("reveal-copy")).toContain("&:focus-within .copy-quiet");
  });

  it("floats over the corner of a message rather than taking a row under it", () => {
    // A row that is invisible until it is hovered still pushes the words above it up, which is the band of nothing
    // under every answer. It leaves the flow only where a pointer can bring it back, so a touch screen keeps a row
    // it can see.
    const float = utility("copy-float");
    expect(float).toContain("@media (hover: hover)");
    expect(float).toContain("position: absolute");
    // The pill the button stands on is a colour of its own, so it fades with the button rather than sitting there
    // empty, and a row that is only invisible still swallows a click meant for the last word of the message.
    expect(float).toContain("opacity: 0");
    expect(float).toContain("pointer-events: none");
    const revealed = utility("reveal-copy");
    expect(revealed).toContain("&:hover .copy-float");
    expect(revealed).toContain("pointer-events: auto");
  });
});
