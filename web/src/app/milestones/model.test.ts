import { describe, expect, it } from "vitest";
import type { MilestoneView } from "../../api/types/milestones";
import {
  dateWords,
  inProject,
  isoOf,
  overdue,
  percent,
  progressWords,
  projectsWords,
  sorted,
  timeline,
} from "./model";

function milestone(over: Partial<MilestoneView> & { id: number }): MilestoneView {
  return {
    name: `M${over.id}`,
    target_date: "2026-12-12",
    description: "",
    created_at: Date.UTC(2026, 10, 1) / 1000,
    cards: 0,
    done: 0,
    projects: [],
    ...over,
  };
}

describe("progress", () => {
  it("is whole percent, and 0 for a milestone with no card", () => {
    expect(percent(13, 20)).toBe(65);
    expect(percent(1, 3)).toBe(33);
    expect(percent(0, 0)).toBe(0);
    expect(progressWords(13, 20)).toBe("13 of 20 cards");
    expect(progressWords(0, 1)).toBe("0 of 1 card");
  });
  it("is overdue when the date passed with cards open", () => {
    const late = milestone({ id: 1, target_date: "2026-11-01", cards: 2, done: 1 });
    expect(overdue(late, "2026-11-19")).toBe(true);
    expect(overdue({ ...late, done: 2 }, "2026-11-19")).toBe(false);
    expect(overdue({ ...late, target_date: "2026-12-01" }, "2026-11-19")).toBe(false);
  });
});

describe("words", () => {
  it("names a date and adds the year when it is another one", () => {
    expect(dateWords("2026-12-12", "2026-11-19")).toBe("Dec 12");
    expect(dateWords("2027-01-20", "2026-11-19")).toBe("Jan 20, 2027");
    expect(isoOf(Date.UTC(2026, 10, 1) / 1000)).toBe("2026-11-01");
  });
  it("names the projects a milestone holds cards of", () => {
    const project = (id: number, name: string) => ({ id, name, key: name, cards: 1, done: 0 });
    expect(projectsWords(milestone({ id: 1 }))).toBe("No cards yet");
    expect(projectsWords(milestone({ id: 1, projects: [project(1, "Web")] }))).toBe("Web");
    expect(projectsWords(milestone({ id: 1, projects: [project(1, "A"), project(2, "B")] }))).toBe(
      "2 projects",
    );
  });
});

describe("sorting and filtering", () => {
  const list = [
    milestone({ id: 1, name: "Zed", target_date: "2027-01-01", cards: 2, done: 2 }),
    milestone({ id: 2, name: "Alpha", target_date: "2026-12-01", cards: 4, done: 1 }),
  ];
  it("sorts by target date, name or progress", () => {
    expect(sorted(list, "").map((m) => m.id)).toEqual([2, 1]);
    expect(sorted(list, "name").map((m) => m.id)).toEqual([2, 1]);
    expect(sorted(list, "progress").map((m) => m.id)).toEqual([1, 2]);
  });
  it("keeps the milestones that hold cards of a project", () => {
    const withCards = [
      milestone({ id: 1, projects: [{ id: 7, name: "Web", key: "WEB", cards: 1, done: 0 }] }),
      milestone({ id: 2 }),
    ];
    expect(inProject(withCards, "7").map((m) => m.id)).toEqual([1]);
    expect(inProject(withCards, "").length).toBe(2);
  });
});

describe("the timeline", () => {
  it("puts whole months on the axis and a bar from the day a milestone was made to its date", () => {
    const t = timeline(
      [milestone({ id: 1, target_date: "2027-01-31", cards: 4, done: 1 })],
      "2026-11-19",
    );
    expect(t.months.map((m) => m.label)).toEqual(["Nov 2026", "Dec", "Jan 2027"]);
    expect(t.months[0]?.left).toBe(0);
    const bar = t.bars[0];
    expect(bar?.left).toBe(0);
    expect(bar?.progress).toBe(25);
    expect((bar?.left ?? 0) + (bar?.width ?? 0)).toBeCloseTo(100, 5);
    expect(t.today).toBeGreaterThan(0);
    expect(t.today).toBeLessThan(100);
  });
  it("keeps a bar visible for a milestone made on its target date", () => {
    const made = Date.UTC(2026, 10, 19) / 1000;
    const t = timeline(
      [milestone({ id: 1, created_at: made, target_date: "2026-11-19" })],
      "2026-11-19",
    );
    expect(t.bars[0]?.width).toBeGreaterThanOrEqual(1.5);
  });
  it("has an axis even with no milestone", () => {
    const t = timeline([], "2026-11-19");
    expect(t.months.length).toBe(1);
    expect(t.bars).toEqual([]);
  });
});
