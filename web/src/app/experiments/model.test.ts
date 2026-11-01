import { describe, expect, it } from "vitest";
import type { Experiment } from "../../api/types/improve";
import {
  actions,
  briefOf,
  budgetOf,
  change,
  emptyForm,
  number,
  result,
  SUGGESTED_BRIEFS,
  statusTag,
} from "./model";

describe("the numbers", () => {
  it("are shown without trailing zeros", () => {
    expect(number(80)).toBe("80");
    expect(number(0.25)).toBe("0.25");
    expect(number(-3.5)).toBe("-3.5");
    expect(number(1 / 3)).toBe("0.3333");
  });

  it("change by a percent of the baseline", () => {
    expect(change(100, 80)).toBe("-20 %");
    expect(change(100, 120)).toBe("20 %");
    expect(change(372, 301)).toBe("-19.1 %");
    expect(change(0, 1)).toBeNull();
  });
});

describe("an experiment", () => {
  const experiment = (extra: Partial<Experiment>): Experiment => ({
    id: 1,
    branch: "research/1-x",
    title: "x",
    value: 301,
    kept: false,
    ...extra,
  });

  it("is kept, not kept or failed", () => {
    expect(result(experiment({ kept: true })).label).toBe("Kept");
    expect(result(experiment({})).label).toBe("Not kept");
    expect(result(experiment({ value: null }))).toEqual({ label: "Failed", tone: "error" });
  });
});

describe("a brief", () => {
  it("needs a repository, a goal, a measure and a budget above 0", () => {
    const form = { ...emptyForm(1), goal: "Faster", measure: "cat score" };
    expect(briefOf(emptyForm(1))).toBeNull();
    expect(briefOf({ ...form, repository: null })).toBeNull();
    expect(briefOf({ ...form, goal: " " })).toBeNull();
    expect(briefOf({ ...form, budget: "0" })).toBeNull();
    expect(briefOf({ ...form, budget: "lots" })).toBeNull();
    expect(briefOf({ ...form, budget: "1000", better: "higher" })).toEqual({
      goal: "Faster",
      measure: "cat score",
      better: "higher",
      token_budget: 1000,
    });
    expect(budgetOf(emptyForm())).toBe(200_000);
  });

  it("starts from any of the points the empty page offers", () => {
    expect(SUGGESTED_BRIEFS.length).toBeGreaterThan(1);
    for (const suggested of SUGGESTED_BRIEFS) {
      const brief = briefOf({ ...emptyForm(1), ...suggested });
      expect(brief).not.toBeNull();
      expect(brief?.goal).toBe(suggested.goal);
      expect(brief?.better).toBe(suggested.better);
    }
    const goals = SUGGESTED_BRIEFS.map((each) => each.goal);
    expect(new Set(goals).size).toBe(goals.length);
  });
});

describe("what a status allows", () => {
  it("is confirm or drop, start or drop, stop, and nothing once ended", () => {
    expect(actions("draft").map((a) => a.action)).toEqual(["confirm", "stop"]);
    expect(actions("confirmed").map((a) => a.label)).toEqual(["Start now", "Drop"]);
    expect(actions("running").map((a) => a.label)).toEqual(["Stop"]);
    for (const status of ["done", "spent", "stopped", "failed"] as const) {
      expect(actions(status)).toEqual([]);
    }
    expect(statusTag("running")).toEqual({ label: "Running", tone: "progress" });
  });
});
