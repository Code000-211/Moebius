/**
 * The calls of the Workflows pages: each asks its route and returns the shape the pages read. A name with a space, or
 * a character a browser escapes, must arrive as the name the workflow has, not as the escaped text.
 */
import { afterEach, describe, expect, it, vi } from "vitest";
import fixtures from "../fixtures/workflows.json";
import { workflow, workflowRun, workflowRuns, workflows } from "./workflows";

function answer(body: unknown, status = 200) {
  return Promise.resolve(new Response(JSON.stringify(body), { status }));
}

afterEach(() => {
  vi.unstubAllGlobals();
});

describe("workflows", () => {
  it("reads the list", async () => {
    const fetch = vi.fn(() => answer(fixtures.list));
    vi.stubGlobal("fetch", fetch);
    await expect(workflows()).resolves.toEqual(fixtures.list);
    expect(fetch).toHaveBeenCalledWith("/v1/workflows", expect.anything());
  });

  it("reads one definition, escaping its name", async () => {
    const fetch = vi.fn(() => answer(fixtures.show));
    vi.stubGlobal("fetch", fetch);
    await expect(workflow("night build")).resolves.toEqual(fixtures.show);
    expect(fetch).toHaveBeenCalledWith("/v1/workflows/night%20build", expect.anything());
  });

  it("reads the runs of one workflow, and one run", async () => {
    const fetch = vi.fn(() => answer(fixtures.runs));
    vi.stubGlobal("fetch", fetch);
    await expect(workflowRuns("night build")).resolves.toEqual(fixtures.runs);
    expect(fetch).toHaveBeenCalledWith(
      "/v1/workflow-runs?workflow=night%20build",
      expect.anything(),
    );
    vi.stubGlobal(
      "fetch",
      vi.fn(() => answer(fixtures.run)),
    );
    await expect(workflowRun(7)).resolves.toEqual(fixtures.run);
  });
});
