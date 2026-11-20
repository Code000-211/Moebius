import { describe, expect, it } from "vitest";
import { filterWords } from "./model";

const NONE = {
  projects: [],
  repositories: [],
  labels: [],
  assignees: [],
  priorities: [],
  cycles: [],
  milestones: [],
  columns: [],
  authors: [],
  agents: [],
  statuses: [],
  due: "",
  text: "",
};

describe("a filter in words", () => {
  it("names the repository and the project, not their ids", () => {
    const names = {
      projects: new Map([[1, "Storefront"]]),
      repositories: new Map([[2, "shop-api"]]),
    };
    expect(
      filterWords({ ...NONE, projects: [1], repositories: [2], priorities: ["high"] }, names),
    ).toBe("project Storefront · repository shop-api · priority high");
    // A name that is not known yet is the id, until the lists are read.
    expect(filterWords({ ...NONE, repositories: [9] }, names)).toBe("repository 9");
  });
});
