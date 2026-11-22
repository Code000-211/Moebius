/**
 * The calls of the Workflows pages: each asks its route and returns the shape the pages read. A name with a space, or
 * a character a browser escapes, must arrive as the name the workflow has, not as the escaped text.
 */
import { afterEach, describe, expect, it, vi } from "vitest";
import fixtures from "../fixtures/workflows.json";
import {
  deleteProjectWorkflow,
  projectWorkflow,
  projectWorkflowRuns,
  projectWorkflows,
  saveProjectWorkflow,
  startProjectWorkflowRun,
  workflowRun,
} from "./workflows";

function answer(body: unknown, status = 200) {
  return Promise.resolve(new Response(JSON.stringify(body), { status }));
}

afterEach(() => {
  vi.unstubAllGlobals();
});

describe("a run", () => {
  it("is read by its id", async () => {
    vi.stubGlobal(
      "fetch",
      vi.fn(() => answer(fixtures.run)),
    );
    await expect(workflowRun(7)).resolves.toEqual(fixtures.run);
  });
});

describe("a project's workflows", () => {
  it("asks the routes of the project, escaping the name", async () => {
    const fetch = vi.fn(() => answer(fixtures.list));
    vi.stubGlobal("fetch", fetch);
    await projectWorkflows(3);
    expect(fetch).toHaveBeenLastCalledWith("/v1/projects/3/workflows", expect.anything());
    await projectWorkflow(3, "night build");
    expect(fetch).toHaveBeenLastCalledWith(
      "/v1/projects/3/workflows/night%20build",
      expect.anything(),
    );
    await projectWorkflowRuns(3, "release");
    expect(fetch).toHaveBeenLastCalledWith(
      "/v1/workflow-runs?project=3&workflow=release",
      expect.anything(),
    );
    await projectWorkflowRuns(3);
    expect(fetch).toHaveBeenLastCalledWith("/v1/workflow-runs?project=3", expect.anything());
  });

  it("saves, deletes and starts through PUT, DELETE and POST", async () => {
    const fetch = vi.fn((..._args: unknown[]) => answer({}));
    vi.stubGlobal("fetch", fetch);
    const definition = fixtures.show.definition;
    await saveProjectWorkflow(3, "release", definition);
    await deleteProjectWorkflow(3, "release");
    await startProjectWorkflowRun(3, "release", { inputs: {} });
    const seen = fetch.mock.calls.map((call) => [call[0], (call[1] as { method?: string }).method]);
    expect(seen).toEqual([
      ["/v1/projects/3/workflows/release", "PUT"],
      ["/v1/projects/3/workflows/release", "DELETE"],
      ["/v1/projects/3/workflows/release/runs", "POST"],
    ]);
  });
});
