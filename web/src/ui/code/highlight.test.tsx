// @vitest-environment jsdom
/**
 * The highlighting of a block of code: a block names its language, the parser of that language tokenises it, and the
 * palette's classes (tok-*) come over the tree. A name the map does not carry, or no name at all, is one plain token;
 * the words are the same either way, so Copy copies what was written.
 */
import { render } from "@testing-library/react";
import { describe, expect, it } from "vitest";
import CodeHighlight from "./CodeHighlight";
import { highlight, parserOf } from "./highlight";

describe("parserOf", () => {
  it("serves Luau with Lua, a diff with its mode, and normalises the name", () => {
    expect(parserOf("luau")).not.toBeNull();
    expect(parserOf("diff")).not.toBeNull();
    expect(parserOf("TOML")).not.toBeNull();
    expect(parserOf(" rust ")).not.toBeNull();
    expect(parserOf("whpl")).toBeNull();
    expect(parserOf()).toBeNull();
    expect(parserOf("Plain text")).toBeNull();
  });
});

describe("highlight", () => {
  it("puts the palette's classes over a Rust block and keeps every word", () => {
    const code = 'fn main() {\n    println!("Answer");\n}\n';
    const tokens = highlight(code, "rust");
    expect(tokens.map((token) => token.text).join("")).toBe(code);
    const words = (classOf: string) =>
      tokens.flatMap((token) => (token.classes === classOf ? [token.text] : []));
    expect(words("tok-keyword")).toContain("fn");
    expect(words("tok-string")).toContain('"Answer"');
    expect(words("tok-title")).toContain("println");
  });

  it("highlights a Luau block the way Lua is tokenised", () => {
    const tokens = highlight('local x = "hi"\n', "luau");
    const words = (classOf: string) =>
      tokens.flatMap((token) => (token.classes === classOf ? [token.text] : []));
    expect(words("tok-keyword")).toContain("local");
    expect(words("tok-string")).toContain('"hi"');
  });

  it("colours a diff by the sign of its lines, whatever case the name came in", () => {
    const diff = "context\n+added\n-removed\n";
    const tokens = highlight(diff, "DIFF");
    expect(tokens.some((token) => token.classes === "tok-add")).toBe(true);
    expect(tokens.some((token) => token.classes === "tok-del")).toBe(true);
  });

  it("draws a block of a language it does not know, and one without a name, plain", () => {
    for (const language of [undefined, "whpl"]) {
      const tokens = highlight("const x = 1", language);
      expect(tokens).toEqual([{ text: "const x = 1", classes: "" }]);
    }
  });
});

describe("CodeHighlight", () => {
  it("draws the tokens as spans and leaves the plain words as text", () => {
    const { container } = render(<CodeHighlight code={'let n = "hi";\n'} language="js" />);
    expect(container.querySelector(".tok-keyword")?.textContent).toBe("let");
    expect(container.querySelector(".tok-string")?.textContent).toBe('"hi"');
    expect(container.querySelector("code")?.className).toContain("whitespace-pre");
  });
});
