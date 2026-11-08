/**
 * The calls of the Workflows pages: each asks its route and returns the shape the pages read. A name with a space, or
 * a character a browser escapes, must arrive as the name the workflow has, not as the escaped text.
 */
import { afterEach, describe, expect, it, vi } from "vitest";
import { workflow, workflowRun, workflows } from "./workflows";

function answer(body: unknown, status = 200) {
  return Promise.resolve(new Response(JSON.stringify(body), { status }));
}

afterEach(() => {
  vi.unstubAllGlobals();
});

describe("workflows", () => {
  it("reads the list", async () => {
    const list = [
      {
        name: "release",
        description: "Build and publish",
        trigger: { kind: "schedule", detail: "0 6 * * 1" },
        node_count: 3,
        last_run: null,
      },
    ];
    const fetch = vi.fn(() => answer(list));
    vi.stubGlobal("fetch", fetch);
    await expect(workflows()).resolves.toEqual(list);
    expect(fetch).toHaveBeenCalledWith("/v1/workflows", expect.anything());
  });

  it("reads one definition, escaping its name", async () => {
    const definition = {
      name: "night build",
      description: "",
      trigger: { kind: "manual", detail: null },
      node_count: 0,
      last_run: null,
      nodes: [],
      edges: [],
      file: "",
    };
    const fetch = vi.fn(() => answer(definition));
    vi.stubGlobal("fetch", fetch);
    await expect(workflow("night build")).resolves.toEqual(definition);
    expect(fetch).toHaveBeenCalledWith("/v1/workflows/night%20build", expect.anything());
  });

  it("reads one run", async () => {
    const run = { id: 7, workflow: "release", status: "failed", nodes: [] };
    const fetch = vi.fn(() => answer(run));
    vi.stubGlobal("fetch", fetch);
    await expect(workflowRun(7)).resolves.toEqual(run);
    expect(fetch).toHaveBeenCalledWith("/v1/workflow-runs/7", expect.anything());
  });
});
