import { describe, expect, it } from "vitest";
import {
  buildTree,
  categoriesOf,
  categoryOf,
  folderOf,
  foldersOf,
  frontMatterOf,
  groupOf,
  headings,
  joinPath,
  labelProblem,
  labelsInUse,
  pagesOfGroup,
  pathProblem,
  splitFolder,
  withLabels,
} from "./model";

const page = (path: string, labels: string[] = []) => ({
  path,
  name: path.split("/").at(-1) ?? path,
  labels,
});

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

describe("categories", () => {
  const pages = [
    page("Handbook"),
    page("Runbooks/Deploy"),
    page("Runbooks/Old/Rollback"),
    page("projects/WEB/Notes"),
    page("projects/WEB/Design/Tokens"),
  ];

  it("tells the group and the category of a page", () => {
    expect(groupOf("Handbook")).toBe("");
    expect(groupOf("Runbooks/Deploy")).toBe("");
    expect(groupOf("projects/WEB/Design/Tokens")).toBe("projects/WEB");
    expect(categoryOf("Handbook")).toBe("");
    expect(categoryOf("Runbooks/Old/Rollback")).toBe("Runbooks/Old");
    expect(categoryOf("projects/WEB/Notes")).toBe("");
    expect(categoryOf("projects/WEB/Design/Tokens")).toBe("Design");
  });

  it("lists the categories of one group, a folder's own folders too", () => {
    expect(categoriesOf(pages, "")).toEqual(["Runbooks", "Runbooks/Old"]);
    expect(categoriesOf(pages, "projects/WEB")).toEqual(["Design"]);
    expect(categoriesOf(pages, "projects/NONE")).toEqual([]);
    expect(pagesOfGroup(pages, "projects/WEB").map((each) => each.path)).toEqual([
      "projects/WEB/Notes",
      "projects/WEB/Design/Tokens",
    ]);
  });

  it("puts a new page in the group and category its address names", () => {
    expect(splitFolder("")).toEqual({ group: "", category: "" });
    expect(splitFolder("Runbooks")).toEqual({ group: "", category: "Runbooks" });
    expect(splitFolder("projects/WEB")).toEqual({ group: "projects/WEB", category: "" });
    expect(splitFolder("projects/WEB/Design")).toEqual({
      group: "projects/WEB",
      category: "Design",
    });
    expect(joinPath("projects/WEB", "", "Notes")).toBe("projects/WEB/Notes");
    expect(joinPath("", "Runbooks", "Deploy")).toBe("Runbooks/Deploy");
  });

  it("counts the pages of a category, the folders in it included", () => {
    const tree = buildTree(pages, []);
    const runbooks = tree.organisation.find((node) => node.name === "Runbooks");
    expect(runbooks?.kind === "folder" && runbooks.count).toBe(2);
    const web = buildTree(pages, [{ key: "WEB", name: "Web" }]).projects[0]?.nodes ?? [];
    const design = web.find((node) => node.name === "Design");
    expect(design?.kind === "folder" && design.count).toBe(1);
  });
});

describe("labels", () => {
  const labelled = [
    page("A", ["Runbook", "On call"]),
    page("B", ["Runbook"]),
    page("C"),
    page("projects/WEB/D", ["Design"]),
  ];

  it("lists the labels in use with their counts", () => {
    expect(labelsInUse(labelled)).toEqual([
      { label: "Design", count: 1 },
      { label: "On call", count: 1 },
      { label: "Runbook", count: 2 },
    ]);
    expect(labelsInUse(pagesOfGroup(labelled, "projects/WEB"))).toEqual([
      { label: "Design", count: 1 },
    ]);
  });

  it("filters to the pages that carry every chosen label", () => {
    const paths = (chosen: string[]) => withLabels(labelled, chosen).map((each) => each.path);
    expect(paths([])).toEqual(["A", "B", "C", "projects/WEB/D"]);
    expect(paths(["Runbook"])).toEqual(["A", "B"]);
    expect(paths(["Runbook", "On call"])).toEqual(["A"]);
    expect(paths(["Nothing"])).toEqual([]);
  });

  it("checks a label as the server does", () => {
    expect(labelProblem("On call")).toBeNull();
    expect(labelProblem("  ")).not.toBeNull();
    expect(labelProblem("a,b")).not.toBeNull();
    expect(labelProblem("a: b")).not.toBeNull();
    expect(labelProblem("x".repeat(41))).not.toBeNull();
    expect(labelProblem("x".repeat(40))).toBeNull();
  });
});

describe("front matter", () => {
  it("is the text before the body", () => {
    expect(frontMatterOf({ text: "---\nlabels: [A]\n---\n# P\n", body: "# P\n" })).toBe(
      "---\nlabels: [A]\n---\n",
    );
    expect(frontMatterOf({ text: "# P\n", body: "# P\n" })).toBe("");
    expect(frontMatterOf({ text: "---\nlabels: [A]\n---\n", body: "" })).toBe(
      "---\nlabels: [A]\n---\n",
    );
  });
});
