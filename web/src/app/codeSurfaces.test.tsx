// @vitest-environment jsdom
/**
 * The boxes of code that sit on the dark colour of code in the board and the Retro: a script shown in a card, the
 * problem a script editor prints and a Retro diff. Their words are drawn with the colour of code text, never the page's
 * text colour, which is dark in the light theme and unreadable on them.
 */
import { readFileSync } from "node:fs";
import { cleanup, render } from "@testing-library/react";
import { afterEach, describe, expect, it } from "vitest";
import { Diff } from "./retro/parts";

afterEach(cleanup);

describe("boxes of code", () => {
  it("draws a Retro diff in the colour of code text", () => {
    const { container } = render(<Diff text={"-old\n+new"} />);
    const pre = container.querySelector("pre");
    expect(pre?.className).toContain("bg-code-bg");
    expect(pre?.className).toContain("text-code-text");
    expect(pre?.className).not.toMatch(/text-text/);
  });

  it.each(["board/CardDetails.tsx", "board/ScriptEditor.tsx"])(
    "keeps %s's code on the dark colour",
    (file) => {
      const source = readFileSync(new URL(file, import.meta.url), "utf8");
      expect(source).toContain("bg-code-bg");
      expect(source).not.toMatch(/bg-code\b(?!-)/);
      expect(source).not.toMatch(/bg-code-bg[^"]*text-text/);
      expect(source).toMatch(/bg-code-bg[^"]*text-code-text/);
    },
  );
});
