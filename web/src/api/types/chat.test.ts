import { describe, expect, it } from "vitest";
import {
  type AvailableModels,
  listedLevels,
  offeredLevels,
  sentLevel,
  THINKING_LEVELS,
} from "./chat";

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

  it("keeps only the levels an endpoint names", () => {
    // No `high` and no `max` here, so neither is offered, and `none` is the quietest level Moebius calls `off`.
    expect(offeredLevels(["none", "minimal", "low", "medium", "xhigh"])).toEqual([
      "off",
      "minimal",
      "low",
      "medium",
      "xhigh",
    ]);
    expect(offeredLevels(["medium", "max"])).toEqual(["medium", "max"]);
    // A name that is not one of Moebius's levels is not offered, though it is still what a level is sent as.
    expect(offeredLevels(["quiet", "loud", "medium"])).toEqual(["medium"]);
  });
});

describe("sentLevel", () => {
  it("names what an endpoint is asked for", () => {
    const levels = ["none", "minimal", "low", "medium", "xhigh"];
    expect(sentLevel(levels, "low")).toBe("low");
    expect(sentLevel(levels, "off")).toBe("none");
    // The endpoint has nothing for the two hardest, so both ask it for its hardest.
    expect(sentLevel(levels, "high")).toBe("xhigh");
    expect(sentLevel(levels, "max")).toBe("xhigh");
    // An endpoint that says nothing is sent the name of the level.
    expect(sentLevel(undefined, "max")).toBe("max");
    expect(sentLevel([], "max")).toBe("max");
  });
});
