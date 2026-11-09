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
import { newWorkflowDraft } from "./model";
import { WorkflowCanvasPage } from "./WorkflowCanvasPage";

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

function show() {
  saves = [];
  vi.stubGlobal(
    "fetch",
    vi.fn(async (input: RequestInfo | URL, init?: RequestInit) => {
      const url = String(input);
      if (init?.method === "PUT" && url === "/v1/workflows/nightly") {
        saves.push(init);
        return { ok: true, status: 200, text: async () => JSON.stringify(SAVED) };
      }
      if (url.endsWith("/workflow-runs/nightly")) {
        return { ok: true, status: 200, text: async () => JSON.stringify([]) };
      }
      throw new Error(`unexpected call ${init?.method ?? "GET"} ${url}`);
    }),
  );
  const client = new QueryClient({ defaultOptions: { queries: { retry: false } } });
  // A data router, which `useBlocker` — the ask-before-leaving — needs.
  const router = createMemoryRouter(
    [
      { path: "/workflows", element: <WorkflowsProbe /> },
      { path: "/workflows/:name/canvas", element: <WorkflowCanvasPage /> },
    ],
    {
      initialEntries: [
        {
          pathname: "/workflows/nightly/canvas",
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
  it("starts with the manual trigger and counts as unsaved, and leads back to the list", async () => {
    show();
    await screen.findByTestId("canvas-page");
    expect(screen.getAllByTestId("canvas-node").length).toBe(1);
    expect(screen.getByTestId("canvas-state").textContent).toBe("Unsaved changes");
    expect(screen.getByRole("link", { name: "nightly" }).getAttribute("href")).toBe("/workflows");
    // No read of a workflow that does not exist was made.
    expect(fetch).not.toHaveBeenCalledWith("/v1/workflows/nightly", expect.anything());
  });

  it("saves under the new name, and then leads back to the workflow", async () => {
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
    expect(screen.getByRole("link", { name: "nightly" }).getAttribute("href")).toBe(
      "/workflows/nightly",
    );
  });
});
