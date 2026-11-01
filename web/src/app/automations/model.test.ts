import { describe, expect, it } from "vitest";
import type { Automation } from "../../api/types/automations";
import { formOf, newForm, nextLine, placeOf, request, scheduleName, until } from "./model";

const NOW = 1_790_000_000;

function automation(extra: Partial<Automation> = {}): Automation {
  return {
    id: 1,
    name: "Morning triage",
    schedule: "0 8 * * 1-5",
    agent: "Moebius",
    prompt: "Triage.",
    repository_id: 1,
    harness_repository: false,
    enabled: true,
    next_firing: NOW + 3 * 3600,
    last_firing: null,
    ...extra,
  };
}

describe("a schedule", () => {
  it("is called by its preset or is the expression", () => {
    expect(scheduleName("0 8 * * 1-5")).toBe("Weekdays at 8");
    expect(scheduleName(" 0 8 * * 1-5 ")).toBe("Weekdays at 8");
    expect(scheduleName("15 3 * * 0")).toBe("15 3 * * 0");
  });

  it("is due in minutes, hours or days", () => {
    expect(until(NOW, NOW + 30)).toBe("in under a minute");
    expect(until(NOW, NOW + 360)).toBe("in 6 min");
    expect(until(NOW, NOW + 3 * 3600)).toBe("in 3 h");
    expect(until(NOW, NOW + 2 * 86_400)).toBe("in 2 d");
    expect(until(NOW, NOW - 5)).toBe("in under a minute");
  });
});

describe("where the chats work", () => {
  it("is a repository, the harness or an empty directory", () => {
    expect(placeOf(automation())).toBe("repository:1");
    expect(placeOf(automation({ repository_id: null, harness_repository: true }))).toBe("harness");
    expect(placeOf(automation({ repository_id: null }))).toBe("nowhere");
  });
});

describe("a save", () => {
  it("says what is missing and sends nothing", () => {
    const form = newForm();
    expect(form.schedule).toBe("0 8 * * *");
    expect(request(form)).toEqual({ problem: "Give the automation a name." });
    expect(request({ ...form, name: "x", schedule: "0 8 * *" })).toEqual({
      problem:
        "A schedule has five fields: minute, hour, day of the month, month, day of the week.",
    });
    expect(request({ ...form, name: "x" })).toEqual({
      problem: "Write the prompt its chats start with.",
    });
  });

  it("sends the trimmed fields and the place", () => {
    const form = { ...formOf(automation()), name: " Triage ", prompt: " Go. " };
    expect(request(form)).toEqual({
      ok: {
        name: "Triage",
        schedule: "0 8 * * 1-5",
        agent: "Moebius",
        prompt: "Go.",
        repository: 1,
        harness: false,
        enabled: true,
      },
    });
    const harness = request({ ...form, place: "harness" });
    expect(harness).toMatchObject({ ok: { repository: null, harness: true } });
  });
});

describe("the line under the schedule", () => {
  it("says when it fires next, once saved and unchanged", () => {
    const saved = automation();
    expect(nextLine(undefined, "0 8 * * *", true, NOW)).toBe(
      "Fires once saved, when the schedule comes due.",
    );
    expect(nextLine(saved, "0 8 * * 1-5", true, NOW)).toBe("Fires next in 3 h.");
    expect(nextLine(saved, "0 9 * * 1-5", true, NOW)).toBe("Save to see when it fires next.");
    expect(nextLine(saved, "0 8 * * 1-5", false, NOW)).toBe("Save to see when it fires next.");
    expect(
      nextLine(automation({ enabled: false, next_firing: null }), "0 8 * * 1-5", false, NOW),
    ).toBe('Off: only "Run now" fires it.');
    expect(nextLine(automation({ next_firing: null }), "0 8 * * 1-5", true, NOW)).toBe(
      "This schedule never comes due.",
    );
  });
});
