import { describe, expect, it } from "vitest";
import type { AvailableModels } from "../api/types/chat";
import { levelChoices } from "./levels";

const available = (over: Partial<AvailableModels>): AvailableModels => ({
  models: [],
  errors: [],
  classes: [],
  decision_models: [],
  ...over,
});

// An endpoint whose hardest level is `xhigh`, with nothing for `high` or `max`.
const listed = available({
  models: [
    {
      id: "one",
      protocol: "openai",
      upstream: "u",
      levels: ["none", "minimal", "low", "medium", "xhigh"],
    },
  ],
});

describe("levelChoices", () => {
  it("offers only what the model takes", () => {
    expect(levelChoices(listed, "one", "").map((option) => option.value)).toEqual([
      "off",
      "minimal",
      "low",
      "medium",
      "xhigh",
    ]);
  });

  it("keeps a level the model has no name for, saying what goes out", () => {
    const options = levelChoices(listed, "one", "max");
    const kept = options[options.length - 1];
    expect(kept).toEqual({ value: "max", label: "max (sent as xhigh)" });
  });

  it("keeps every level of a model whose upstream does not say", () => {
    expect(levelChoices(available({}), "one", "")).toHaveLength(7);
  });
});
