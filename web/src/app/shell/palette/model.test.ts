import { describe, expect, it } from "vitest";
import { rank, ranked } from "./model";

describe("how a query ranks what it finds", () => {
  it("puts a whole word above the start of a word above a place inside one", () => {
    expect(rank("tom", "Tom Baker person")).toBe(3);
    expect(rank("tom", "Tomatoes")).toBe(2);
    expect(rank("tom", "Automations page")).toBe(1);
    expect(rank("tom", "Agents page")).toBe(0);
    expect(rank("", "anything")).toBe(3);
  });

  it("sorts the better matches first and keeps the order of equals", () => {
    const names = ["Automations", "Tom", "Atomic", "Tomorrow"];
    expect(ranked("tom", names, (n) => n)).toEqual(["Tom", "Tomorrow", "Automations", "Atomic"]);
  });
});
