/**
 * A count and its noun agree, and the digits are said once: a page that groups a big number by thousands writes the
 * number itself and asks for the word, so a helper that returns both would print "1 240 1,240 calls".
 */
import { describe, expect, it } from "vitest";
import { ago, dayName, elapsed, noun, plural } from "./format";

describe("how long ago", () => {
  it("is the unit a person reads, in the one shape every list uses", () => {
    expect(ago(100, 90)).toBe("just now");
    expect(ago(1000, 700)).toBe("5 min ago");
    expect(ago(10_000, 0)).toBe("2 h ago");
    expect(ago(200_000, 0)).toBe("2 d ago");
    expect(ago(1000, 2000)).toBe("just now");
  });
});

describe("a count and its noun", () => {
  it("agrees", () => {
    expect(plural(1, "run", "runs")).toBe("1 run");
    expect(plural(3, "run", "runs")).toBe("3 runs");
    expect(plural(0, "skill", "skills")).toBe("0 skills");
  });

  it("leaves the writing of the number to the page that groups it", () => {
    expect(noun(1, "call", "calls")).toBe("call");
    expect(noun(1240, "call", "calls")).toBe("calls");
    expect(`1,240 ${noun(1240, "call", "calls")}`).toBe("1,240 calls");
  });
});

/**
 * A step that has finished is labelled with how long it took — `Thought 12s` — so every line that measures something
 * says it the same short way: seconds while they last, then minutes, then hours. A length that has not happened yet
 * is not a length, so a gap counted backwards reads as no time rather than as a negative one.
 */
describe("how long something took", () => {
  it("counts seconds, then minutes, then hours", () => {
    expect(elapsed(0)).toBe("0s");
    expect(elapsed(12)).toBe("12s");
    expect(elapsed(59.4)).toBe("59s");
    expect(elapsed(60)).toBe("1m");
    expect(elapsed(125)).toBe("2m");
    expect(elapsed(3_599)).toBe("59m");
    expect(elapsed(3_600)).toBe("1h");
    expect(elapsed(9_000)).toBe("2h");
  });

  it("is the words a finished thought is labelled with", () => {
    expect(`Thought ${elapsed(12)}`).toBe("Thought 12s");
  });

  it("never counts back from zero", () => {
    expect(elapsed(-3)).toBe("0s");
  });
});

/**
 * A day is named for where it sits next to now, and only becomes a date once it is far enough away that a person
 * would rather read it than count back to it.
 */
describe("a day in words", () => {
  it("names the two nearest days", () => {
    expect(dayName("2026-10-31", "2026-10-31")).toBe("Today");
    expect(dayName("2026-10-30", "2026-10-31")).toBe("Yesterday");
  });

  it("writes the day itself from two days back, with the year once it leaves this one", () => {
    expect(dayName("2026-10-29", "2026-10-31")).toBe("29 Oct");
    expect(dayName("2026-01-05", "2026-10-31")).toBe("5 Jan");
    expect(dayName("2025-12-31", "2026-10-31")).toBe("31 Dec 2025");
  });

  it("crosses a month and a year without counting days twice", () => {
    expect(dayName("2026-03-01", "2026-03-02")).toBe("Yesterday");
    expect(dayName("2025-12-31", "2026-01-01")).toBe("Yesterday");
  });

  it("leaves a day the server did not name alone", () => {
    expect(dayName("", "2026-10-31")).toBe("");
  });
});
