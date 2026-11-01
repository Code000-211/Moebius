import { describe, expect, it } from "vitest";
import type { Lesson, ReportError } from "../../api/types/improve";
import {
  cardsLine,
  groupErrors,
  lessonMeta,
  lessonTag,
  origin,
  ownKind,
  plural,
  proposalTag,
  retroTag,
  trialLine,
} from "./model";

function error(kind: string, extra: Partial<ReportError> = {}): ReportError {
  return { kind, run: null, card: null, agent: null, text: kind, ...extra };
}

describe("a report", () => {
  it("groups its errors by kind in the page's order and leaves out the kinds without one", () => {
    const groups = groupErrors([
      error("review_findings"),
      error("tool_failed"),
      error("steered"),
      error("tool_failed"),
    ]);
    expect(groups.map((g) => [g.label, g.errors.length])).toEqual([
      ["Failed tool calls", 2],
      ["Steering", 1],
      ["Review findings", 1],
    ]);
    expect(groupErrors([])).toEqual([]);
  });

  it("says where an error happened", () => {
    expect(origin(error("x", { card: 48, run: 12, agent: "Coder" }))).toBe(
      "card 48 · run 12 · Coder",
    );
    expect(origin(error("x", { card: 50 }))).toBe("card 50");
    expect(origin(error("x"))).toBe("");
  });
});

describe("the tags", () => {
  it("of a Retro", () => {
    expect(retroTag("proposed")).toEqual({ label: "Proposed", tone: "teal" });
    expect(retroTag("failed")).toEqual({ label: "Failed", tone: "red" });
    expect(retroTag("analysing").label).toBe("Analysing");
    expect(retroTag("reported").label).toBe("Reported");
  });

  it("of a proposal", () => {
    expect(proposalTag("positive")).toEqual({ label: "Positive", tone: "teal" });
    expect(proposalTag("negative").tone).toBe("red");
    expect(proposalTag("untried").tone).toBe("amber");
    expect(proposalTag("open").label).toBe("Open");
    expect(ownKind("settings")).toBe("moebius.toml");
    expect(ownKind("repository")).toBe(".moebius/");
    expect(ownKind("harness")).toBeNull();
    expect(ownKind(undefined)).toBeNull();
  });

  it("of a lesson", () => {
    expect(lessonTag("success").label).toBe("Worked");
    expect(lessonTag("rejection").label).toBe("Rejected");
    expect(lessonTag("failure").label).toBe("Failed");
  });
});

describe("the lines", () => {
  it("count in words", () => {
    expect(plural(1, "error", "errors")).toBe("1 error");
    expect(plural(0, "proposal", "proposals")).toBe("0 proposals");
    expect(cardsLine([48, 50])).toBe("Cards 48, 50");
  });

  it("of a trial name its card", () => {
    expect(trialLine({ card_id: 48, outcome: "positive", reason: "" })).toEqual({
      outcome: { label: "Positive", tone: "teal" },
      card: "Card 48",
    });
    expect(trialLine({ card_id: 7, regression: true, outcome: "negative", reason: "" }).card).toBe(
      "Regression card 7",
    );
    expect(trialLine({ card_id: null, outcome: "skipped", reason: "" })).toEqual({
      outcome: { label: "Skipped", tone: "plain" },
      card: "No card",
    });
  });

  it("of a lesson say its cards, how often it was given and why it was retired", () => {
    const lesson: Lesson = {
      id: 1,
      repository_id: 1,
      kind: "failure",
      situation: "s",
      lesson: "l",
      advice: "a",
      cards: [41],
      status: "retired",
      retired_reason: "not retrieved for 60 days",
      retrievals: 0,
    };
    expect(lessonMeta(lesson)).toBe("Cards 41 · given 0 times · not retrieved for 60 days");
    expect(lessonMeta({ ...lesson, cards: [], retired_reason: null, retrievals: 4 })).toBe(
      "given 4 times",
    );
  });
});
