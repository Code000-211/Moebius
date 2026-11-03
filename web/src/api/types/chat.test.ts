import { describe, expect, it } from "vitest";
import { type AvailableModels, listedLevels, offeredLevels, THINKING_LEVELS } from "./chat";

const available = (over: Partial<AvailableModels>): AvailableModels => ({
  models: [],
  errors: [],
  classes: [],
  decision_models: [],
  ...over,
});

describe("listedLevels", () => {
  it("reads the levels a model lists", () => {
    const listed = available({
      models: [
        { id: "one", protocol: "openai", upstream: "u", levels: ["low", "xhigh"] },
        { id: "two", protocol: "openai", upstream: "u" },
      ],
    });
    expect(listedLevels(listed, "one")).toEqual(["low", "xhigh"]);
    expect(listedLevels(listed, "two")).toBeUndefined();
    expect(listedLevels(listed, "")).toBeUndefined();
    expect(listedLevels(undefined, "one")).toBeUndefined();
  });

  it("joins the levels of the models of a class", () => {
    const listed = available({
      models: [
        { id: "one", protocol: "openai", upstream: "u", levels: ["low", "medium"] },
        { id: "two", protocol: "openai", upstream: "u", levels: ["medium", "xhigh"] },
        { id: "three", protocol: "anthropic", upstream: "u" },
      ],
      classes: [
        { name: "strong", models: ["one", "two", "three"], protocols: ["openai"] },
        { name: "quiet", models: ["three"], protocols: ["anthropic"] },
      ],
    });
    expect(listedLevels(listed, "strong")).toEqual(["low", "medium", "xhigh"]);
    expect(listedLevels(listed, "quiet")).toBeUndefined();
  });
});

describe("offeredLevels", () => {
  it("keeps every level of a model that does not say", () => {
    expect(offeredLevels(undefined)).toEqual([...THINKING_LEVELS]);
    expect(offeredLevels([])).toEqual([...THINKING_LEVELS]);
  });

  it("keeps the levels a model has a name or a place for", () => {
    // An endpoint whose hardest level is `xhigh` has no name for `max`, so `max` is not offered.
    expect(offeredLevels(["none", "minimal", "low", "medium", "xhigh"])).toEqual([
      "off",
      "minimal",
      "low",
      "medium",
      "high",
      "xhigh",
    ]);
    // A model with two names is offered the two easiest, which is where they fall, and any it names itself.
    expect(offeredLevels(["medium", "max"])).toEqual(["off", "minimal", "medium", "max"]);
  });
});
