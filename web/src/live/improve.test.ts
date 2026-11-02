import { describe, expect, it } from "vitest";
import { keys } from "../api/keys";
import { invalidates } from "./invalidation";

const event = (kind: string) => invalidates({ kind, data: {} });

describe("the events of the Rules, Automations, Retro, Experiments and Decision model pages", () => {
  it("make the Retros and the lessons stale when a Retro or a trial ends", () => {
    for (const kind of ["retro_done", "trial_done"]) {
      expect(event(kind)).toContainEqual(keys.retros);
      expect(event(kind)).toContainEqual(keys.lessons);
      expect(event(kind)).toContainEqual(keys.proposals);
    }
    expect(event("proposal_status")).toContainEqual(keys.proposals);
    expect(event("proposal_status")).toContainEqual(keys.retros);
    expect(event("proposal_status")).toContainEqual(keys.rules);
  });

  it("make the research stale when it changes", () => {
    expect(event("research_status")).toEqual([keys.research]);
    expect(event("experiment_done")).toEqual([keys.research]);
  });

  it("make everything of the decision model stale when a takeover changes", () => {
    expect(event("takeover_status")).toContainEqual(["decision-model"]);
    expect(event("takeover_proposed")).toContainEqual(["decision-model"]);
    // The keys of its three reads all start with the key the event names.
    for (const key of [keys.shadowing, keys.kinds, keys.exports])
      expect(key[0]).toBe("decision-model");
  });

  it("make the automations and their firings stale when one fires or a chat ends", () => {
    expect(event("automation_fired")).toEqual([keys.automations]);
    expect(event("run_ended")).toContainEqual(keys.automations);
    expect(keys.firings(3).slice(0, 1)).toEqual(keys.automations);
  });

  it("make the rules stale when the rules file of a repository is saved, and no other file", () => {
    const saved = (path: string) =>
      invalidates({ kind: "file_changed", data: { repository: 1, path } });
    expect(saved(".moebius/rules.md")).toContainEqual(keys.rules);
    expect(saved("src/main.rs")).not.toContainEqual(keys.rules);
  });
});
