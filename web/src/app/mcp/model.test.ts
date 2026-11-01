import { describe, expect, it } from "vitest";
import { blankForm, formOf, mcpRequest } from "./model";

// biome-ignore lint/suspicious/noTemplateCurlyInString: a value may hold a ${NAME} reference, which the server resolves
const TOKEN = "${DOCS_TOKEN}";
// biome-ignore lint/suspicious/noTemplateCurlyInString: as above
const BEARER = "Bearer ${KEY}";

describe("saving an MCP server", () => {
  it("sends a command with its arguments, one per line, and its environment", () => {
    const form = {
      ...blankForm("docs"),
      command: " docs-server ",
      args: "--root\n\n/srv/docs\n",
      rows: [
        ["TOKEN", TOKEN],
        ["", ""],
      ] as Array<[string, string]>,
    };
    expect(mcpRequest(form)).toEqual({
      name: "docs",
      definition: {
        command: "docs-server",
        args: ["--root", "/srv/docs"],
        env: { TOKEN: TOKEN },
      },
    });
  });

  it("sends a URL with its headers and none of the command's keys", () => {
    const form = {
      ...blankForm("search"),
      remote: true,
      command: "left over",
      url: "https://search.example.com/mcp",
      rows: [["Authorization", BEARER]] as Array<[string, string]>,
    };
    expect(mcpRequest(form)).toEqual({
      name: "search",
      definition: {
        url: "https://search.example.com/mcp",
        headers: { Authorization: BEARER },
      },
    });
  });

  it.each([
    [{ name: "no good" }, "A name is made of letters, digits, - and _."],
    [{ command: " " }, "Enter the command."],
    [{ rows: [["", "value"]] as Array<[string, string]>, command: "x" }, "Give each value a name."],
    [{ remote: true, url: "" }, "Enter the URL."],
  ])("says what is wrong with %j", (change, problem) => {
    expect(mcpRequest({ ...blankForm("ok"), ...change })).toBe(problem);
  });
});

describe("opening an MCP server", () => {
  it("fills a command's fields and a URL's", () => {
    expect(formOf("docs", { command: "d", args: ["a", "b"], env: { K: "v" } })).toMatchObject({
      remote: false,
      command: "d",
      args: "a\nb",
      rows: [["K", "v"]],
    });
    expect(formOf("s", { url: "https://x", headers: { H: "v" } })).toMatchObject({
      remote: true,
      url: "https://x",
      rows: [["H", "v"]],
    });
  });
});
