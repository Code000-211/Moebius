/**
 * Tests of the lists the board filters by.
 */
import { describe, expect, it } from "vitest";
import { board, card, run } from "./fixtures";
import { agentsOf } from "./index";

describe("agentsOf", () => {
  it("lists each agent that has a run, sorted", () => {
    const b = board([
      card(1, { run }),
      card(2, { run: { ...run, agent: "Analyst" } }),
      card(3, { run }),
      card(4),
    ]);
    expect(agentsOf(b)).toEqual(["Analyst", "Coder"]);
  });
});
