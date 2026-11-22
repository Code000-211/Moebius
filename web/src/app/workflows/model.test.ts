import { describe, expect, it } from "vitest";
import type { Automation } from "../../api/types/automations";
import type { LastRun, WorkflowSummary } from "../../api/types/workflows";
import {
  lastRunLine,
  needsAttention,
  runFilter,
  runStatus,
  scheduledWorkflows,
  triggerName,
} from "./model";

describe("a trigger", () => {
  it("says what starts the workflow", () => {
    expect(triggerName("manual")).toBe("Manual");
    expect(triggerName("schedule")).toBe("Schedule");
    expect(triggerName("event")).toBe("Event");
    expect(triggerName("webhook")).toBe("Webhook");
  });

  it("keeps a kind it does not know, and says a workflow with none has no trigger", () => {
    expect(triggerName("queue")).toBe("queue");
    expect(triggerName(null)).toBe("No trigger");
    expect(triggerName("")).toBe("No trigger");
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
  const run = (status: string): LastRun => ({
    id: 7,
    status,
    created_at: 0,
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

describe("the filter of the runs", () => {
  it("is what the address names, and all of them otherwise", () => {
    expect(runFilter("scheduled")).toBe("scheduled");
    expect(runFilter("failed")).toBe("failed");
    expect(runFilter("everything")).toBe("all");
    expect(runFilter(null)).toBe("all");
  });
});

describe("the scheduled workflows", () => {
  const summary = (name: string, trigger: string): WorkflowSummary => ({
    project: 1,
    project_name: "demo",
    name,
    description: "",
    trigger_kind: trigger,
    node_count: 1,
    last_run: null,
  });
  const automation = (id: number, name: string): Automation => ({
    id,
    name,
    schedule: "0 8 * * *",
    agent: "Moebius",
    prompt: "Go.",
    repository_id: null,
    project_id: 1,
    harness_repository: false,
    enabled: true,
    next_firing: 10,
    last_firing: null,
  });

  it("are the schedule-trigger workflows, with the automation that is one", () => {
    const found = scheduledWorkflows(
      [
        summary("manual-one", "manual"),
        summary("nightly", "schedule"),
        summary("automation-4", "schedule"),
        summary("plain-schedule", "schedule"),
      ],
      [automation(3, "nightly"), automation(4, "Triage the overnight issues")],
    );
    expect(found.map((each) => [each.workflow.name, each.automation?.id])).toEqual([
      ["nightly", 3],
      ["automation-4", 4],
      ["plain-schedule", undefined],
    ]);
  });
});
