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
  spanOf,
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
  it("lists only the milestones that hold cards of the project, counted for that project alone", () => {
    const web = { id: 7, name: "Web", key: "WEB", cards: 1, done: 0 };
    const api = { id: 8, name: "Api", key: "API", cards: 4, done: 3 };
    const all = [
      milestone({ id: 1, cards: 5, done: 3, projects: [web, api] }),
      milestone({ id: 2, cards: 4, done: 3, projects: [api] }),
      milestone({ id: 3 }),
    ];
    expect(inProject(all, 7).map((m) => [m.id, m.cards, m.done])).toEqual([[1, 1, 0]]);
    expect(inProject(all, 8).map((m) => [m.id, m.cards, m.done])).toEqual([
      [1, 4, 3],
      [2, 4, 3],
    ]);
    expect(inProject(all, 9)).toEqual([]);
  });
  it("lists a milestone made from the tab until a card is in it", () => {
    const all = [milestone({ id: 3 })];
    expect(inProject(all, 7, new Set([3])).map((m) => [m.id, m.cards])).toEqual([[3, 0]]);
    expect(inProject(all, 7, new Set([4]))).toEqual([]);
  });
});

describe("the span of a milestone", () => {
  it("runs from the day it was made to its target date", () => {
    expect(spanOf(milestone({ id: 1, target_date: "2027-01-31" }))).toEqual({
      start: "2026-11-01",
      end: "2027-01-31",
    });
  });
  it("is the target date alone when the milestone was made after it", () => {
    expect(spanOf(milestone({ id: 1, target_date: "2026-10-01" }))).toEqual({
      start: "2026-10-01",
      end: "2026-10-01",
    });
  });
});
