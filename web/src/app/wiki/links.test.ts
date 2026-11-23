import { describe, expect, it } from "vitest";
import {
  cardChoices,
  complete,
  completionAt,
  linkPages,
  linkText,
  pageChoices,
  wikiAddress,
} from "./links";

const page = (path: string) => ({ path, name: path.split("/").at(-1) ?? path, labels: [] });

describe("links in a page's text", () => {
  it("makes a link to the page a name leads to, and leaves the rest as written", () => {
    const links = [
      { link: "On call", path: "Team/On call" },
      { link: "Missing", path: null },
    ];
    expect(linkPages("See [[On call]], [[ On call |the rota]] and [[Missing]].", links)).toBe(
      `See [On call](${wikiAddress("Team/On call")}), [the rota](${wikiAddress("Team/On call")}) and [[Missing]].`,
    );
    expect(linkPages("`[[On call]]` and\n```\n[[On call]]\n```\n", links)).toBe(
      "`[[On call]]` and\n```\n[[On call]]\n```\n",
    );
    expect(linkPages("[[a]b]]", links)).toBe("[[a]b]]");
  });
});

describe("completion while typing", () => {
  it("finds a page after [[ and a card from the start of its name", () => {
    expect(completionAt("See [[On", 8)).toEqual({ kind: "page", query: "On", from: 6 });
    expect(completionAt("a\nSee [[", 8)).toEqual({ kind: "page", query: "", from: 8 });
    expect(completionAt("See [[On call]] and", 19)).toBeNull();
    expect(completionAt("See [[On|la", 11)).toBeNull();
    expect(completionAt("Fix WEB-1", 9)).toEqual({ kind: "card", query: "WEB-1", from: 4 });
    expect(completionAt("Fix WEB-", 8)).toEqual({ kind: "card", query: "WEB-", from: 4 });
    expect(completionAt("Ask WEB", 7)).toBeNull();
    expect(completionAt("Fix web-1", 9)).toBeNull();
    expect(completionAt("Fix a-WEB-1", 11)).toBeNull();
  });

  it("offers pages by name first, and writes the shortest link that leads there", () => {
    const pages = [page("Team/On call"), page("projects/WEB/On call"), page("Handbook")];
    expect(pageChoices(pages, "on").map((each) => each.path)).toEqual([
      "Team/On call",
      "projects/WEB/On call",
    ]);
    expect(pageChoices(pages, "team").map((each) => each.path)).toEqual(["Team/On call"]);
    expect(linkText(page("Handbook"), pages)).toBe("Handbook");
    expect(linkText(page("Team/On call"), pages)).toBe("Team/On call");
    expect(
      cardChoices(
        [
          { key: "WEB-1", title: "a" },
          { key: "API-2", title: "b" },
        ],
        "WE",
      ),
    ).toEqual([{ key: "WEB-1", title: "a" }]);
  });

  it("puts the choice where it was typed and the caret after it", () => {
    const text = "See [[On and more";
    const done = complete(text, 8, { kind: "page", query: "On", from: 6 }, "On call");
    expect(done).toEqual({ text: "See [[On call]] and more", caret: 15 });
    const again = complete("See [[On]] x", 8, { kind: "page", query: "On", from: 6 }, "On call");
    expect(again).toEqual({ text: "See [[On call]] x", caret: 15 });
    const card = complete("Fix WE", 6, { kind: "card", query: "WE", from: 4 }, "WEB-12");
    expect(card).toEqual({ text: "Fix WEB-12", caret: 10 });
  });
});
