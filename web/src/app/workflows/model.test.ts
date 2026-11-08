import { describe, expect, it } from "vitest";
import type { WorkflowRunSummary } from "../../api/workflows";
import { lastRunLine, needsAttention, runStatus, triggerName } from "./model";

describe("a trigger", () => {
  it("says what starts the workflow, with its schedule or event", () => {
    expect(triggerName({ kind: "manual", detail: null })).toBe("Manual");
    expect(triggerName({ kind: "schedule", detail: "0 6 * * 1" })).toBe("Schedule · 0 6 * * 1");
    expect(triggerName({ kind: "event", detail: "issues.opened" })).toBe("Event · issues.opened");
    expect(triggerName({ kind: "webhook", detail: "push" })).toBe("Webhook · push");
  });

  it("keeps a kind it does not know, and says a workflow with none has no trigger", () => {
    expect(triggerName({ kind: "queue", detail: null })).toBe("queue");
    expect(triggerName(null)).toBe("No trigger");
  });
});

describe("a status", () => {
  it("is its label and tone, and a successful run is done", () => {
    expect(runStatus("queued")).toEqual({ label: "Queued", tone: "plain" });
    expect(runStatus("running")).toEqual({ label: "Running", tone: "progress" });
    expect(runStatus("waiting")).toEqual({ label: "Waiting", tone: "attention" });
    expect(runStatus("succeeded")).toEqual({ label: "Done", tone: "complete" });
    expect(runStatus("failed")).toEqual({ label: "Failed", tone: "error" });
    expect(runStatus("cancelled")).toEqual({ label: "Cancelled", tone: "plain" });
    expect(runStatus("skipped")).toEqual({ label: "Skipped", tone: "plain" });
  });

  it("shows a word it does not know as it came", () => {
    expect(runStatus("paused")).toEqual({ label: "paused", tone: "plain" });
  });
});

describe("the line under a workflow's last run", () => {
  const run = (status: string): WorkflowRunSummary => ({
    id: 7,
    status,
    started_at: 1,
    finished_at: 2,
  });

  it("says a workflow that has not run never has", () => {
    expect(lastRunLine(null)).toBe("Never run");
  });

  it("says how the last run went", () => {
    expect(lastRunLine(run("succeeded"))).toBe("Last run: done");
    expect(lastRunLine(run("failed"))).toBe("Last run: failed");
  });
});

describe("what needs attention", () => {
  it("is a run that failed or waits, and nothing else", () => {
    expect(needsAttention("failed")).toBe(true);
    expect(needsAttention("waiting")).toBe(true);
    expect(needsAttention("succeeded")).toBe(false);
    expect(needsAttention(null)).toBe(false);
  });
});
