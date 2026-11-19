import { describe, expect, it } from "vitest";
import { buildTree, folderOf, foldersOf, headings, pathProblem } from "./model";

const page = (path: string) => ({ path, name: path.split("/").at(-1) ?? path });

describe("the tree of the wiki", () => {
  it("puts folders before pages and gives every project a folder of its own", () => {
    const tree = buildTree(
      [
        page("Handbook"),
        page("Team/On call"),
        page("Team/Release"),
        page("projects/SHOP/Architecture/Payments"),
        page("projects/SHOP/Onboarding"),
        page("projects/OLD/Notes"),
      ],
      [
        { key: "SHOP", name: "Storefront" },
        { key: "WEB", name: "Website" },
      ],
    );
    expect(tree.organisation.map((node) => [node.kind, node.path])).toEqual([
      ["folder", "Team"],
      ["page", "Handbook"],
    ]);
    const team = tree.organisation[0];
    expect(team?.kind === "folder" && team.children.map((node) => node.path)).toEqual([
      "Team/On call",
      "Team/Release",
    ]);
    expect(tree.projects.map((project) => [project.key, project.name])).toEqual([
      ["SHOP", "Storefront"],
      ["WEB", "Website"],
      ["OLD", "OLD"],
    ]);
    const shop = tree.projects[0]?.nodes ?? [];
    expect(shop.map((node) => node.path)).toEqual([
      "projects/SHOP/Architecture",
      "projects/SHOP/Onboarding",
    ]);
    expect(tree.projects[1]?.nodes).toEqual([]);
  });

  it("names the folders above a page", () => {
    expect(foldersOf("projects/SHOP/Architecture/Payments")).toEqual([
      "projects",
      "projects/SHOP",
      "projects/SHOP/Architecture",
    ]);
    expect(foldersOf("Handbook")).toEqual([]);
    expect(folderOf("Team/On call")).toBe("Team");
    expect(folderOf("Handbook")).toBe("");
  });
});

describe("the contents of a page", () => {
  it("lists the headings outside code", () => {
    const text = [
      "# Payments",
      "Text",
      "## Rounding ##",
      "```",
      "# not a heading",
      "```",
      "### The *providers*",
      "#### Too deep",
      "#hashtag",
    ].join("\n");
    expect(headings(text)).toEqual([
      { level: 1, text: "Payments" },
      { level: 2, text: "Rounding" },
      { level: 3, text: "The providers" },
    ]);
  });
});

describe("a page's path", () => {
  it("takes plain names and refuses what the server refuses", () => {
    expect(pathProblem("Team/On call", [])).toBeNull();
    expect(pathProblem("projects/SHOP/Payments", ["SHOP"])).toBeNull();
    expect(pathProblem("projects", [])).toBeNull();
    for (const bad of [
      "",
      "a//b",
      "/a",
      ".hidden",
      " a",
      "a:b",
      "projects/SHOP",
      "projects/NOPE/x",
    ]) {
      expect(pathProblem(bad, ["SHOP"]), bad).not.toBeNull();
    }
  });
});
