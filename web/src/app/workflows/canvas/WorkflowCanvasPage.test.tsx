// @vitest-environment jsdom
/**
 * The canvas started from a draft rather than a file: the "New workflow" dialog passes the blank graph as router
 * state, the canvas opens without reading the workflow — there is no file to read — and counts as unsaved until the
 * first save, which writes the workflow under its new name.
 */
import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { cleanup, fireEvent, render, screen, waitFor } from "@testing-library/react";
import { createMemoryRouter, RouterProvider } from "react-router";
import { afterEach, describe, expect, it, vi } from "vitest";
import { keys } from "../../../api/keys";
import { WorkflowPage } from "../WorkflowPage";
import { newWorkflowDraft } from "./model";

afterEach(() => {
  cleanup();
  vi.unstubAllGlobals();
});

// The graph library measures the pane and its cards with a ResizeObserver, which jsdom does not have.
class ResizeObserverStub {
  observe() {}
  unobserve() {}
  disconnect() {}
}
window.ResizeObserver = ResizeObserverStub;

/** Stands in for the list page, so a test sees where the way back leads. */
function WorkflowsProbe() {
  return <div data-testid="workflows-probe" />;
}

const SAVED = {
  name: "nightly",
  file: '[trigger]\nkind = "manual"\n',
  definition: newWorkflowDraft("nightly", "Build the change every night."),
  commit: "0123456789abcdef",
};

/** The PUT the canvas's first save makes, if it has happened yet. */
let saves: RequestInit[];
let client: QueryClient;

function show() {
  saves = [];
  vi.stubGlobal(
    "fetch",
    vi.fn(async (input: RequestInfo | URL, init?: RequestInit) => {
      const url = String(input);
      if (init?.method === "PUT" && url === "/v1/projects/3/workflows/nightly") {
        saves.push(init);
        return { ok: true, status: 200, text: async () => JSON.stringify(SAVED) };
      }
      if (url.includes("/v1/workflow-runs") || url === "/v1/projects") {
        return { ok: true, status: 200, text: async () => JSON.stringify([]) };
      }
      if (url === "/v1/projects/3/workflows" || url === "/v1/projects/3/workflows/nightly") {
        return {
          ok: true,
          status: 200,
          text: async () => JSON.stringify(url.endsWith("nightly") ? SAVED : []),
        };
      }
      throw new Error(`unexpected call ${init?.method ?? "GET"} ${url}`);
    }),
  );
  client = new QueryClient({ defaultOptions: { queries: { retry: false } } });
  // A data router, which `useBlocker` — the ask-before-leaving — needs.
  const router = createMemoryRouter(
    [
      { path: "/projects/:projectId/workflows", element: <WorkflowsProbe /> },
      { path: "/projects/:projectId/workflows/:name/:tab?", element: <WorkflowPage /> },
    ],
    {
      initialEntries: [
        {
          pathname: "/projects/3/workflows/nightly/canvas",
          state: { draft: newWorkflowDraft("nightly", "Build the change every night.") },
        },
      ],
    },
  );
  render(
    <QueryClientProvider client={client}>
      <RouterProvider router={router} />
    </QueryClientProvider>,
  );
}

describe("a canvas opened with a new draft", () => {
  it("reads the automations again after a save, as the Scheduled filter joins against them", async () => {
    show();
    client.setQueryData(keys.automations, []);
    await screen.findByTestId("canvas-page");
    fireEvent.click(screen.getByTestId("canvas-save"));
    await waitFor(() => {
      expect(client.getQueryState(keys.automations)?.isInvalidated).toBe(true);
    });
  });

  it("starts with the manual trigger and counts as unsaved, under a header with the way back", async () => {
    show();
    await screen.findByTestId("canvas-page");
    expect(screen.getAllByTestId("canvas-node").length).toBe(1);
    expect(screen.getByTestId("canvas-state").textContent).toBe("Unsaved changes");
    expect(screen.getByRole("link", { name: "Workflows" }).getAttribute("href")).toBe(
      "/projects/3/workflows",
    );
    // A workflow with no file has no tabs but its canvas.
    expect(screen.queryByRole("tablist", { name: "Workflow" })).toBeNull();
    // No read of a workflow that does not exist was made.
    expect(fetch).not.toHaveBeenCalledWith("/v1/projects/3/workflows/nightly", expect.anything());
  });

  it("saves under the new name, and then the page has its tabs", async () => {
    show();
    await screen.findByTestId("canvas-page");
    fireEvent.click(screen.getByTestId("canvas-save"));
    await waitFor(() => {
      expect(saves).toHaveLength(1);
    });
    expect(saves[0]?.body).toEqual(
      JSON.stringify({
        definition: {
          name: "nightly",
          description: "Build the change every night.",
          trigger: [{ kind: "manual", params: {} }],
          nodes: [],
          edges: [],
        },
      }),
    );
    await waitFor(() => {
      expect(screen.getByTestId("canvas-state").textContent).toBe("Saved as 0123456");
    });
    expect(await screen.findByRole("tablist", { name: "Workflow" })).toBeTruthy();
  });
});
