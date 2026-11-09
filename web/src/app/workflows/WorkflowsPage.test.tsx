/**
 * The Workflows list: what each card says about a workflow and its last run, the link that opens the definition and the
 * link that opens the last run, and the sentence an empty list gives instead of a blank page.
 */
import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { fireEvent, render, screen } from "@testing-library/react";
import { MemoryRouter, Route, Routes, useLocation } from "react-router";
import { afterEach, describe, expect, it, vi } from "vitest";
import { WorkflowsPage } from "./WorkflowsPage";

const LIST = [
  {
    name: "release",
    description: "Build, test and publish a release.",
    trigger: { kind: "schedule", detail: "0 6 * * 1" },
    node_count: 3,
    last_run: { id: 41, status: "failed", started_at: 1, finished_at: 2 },
  },
  {
    name: "triage",
    description: "Sort new issues.",
    trigger: { kind: "manual", detail: null },
    node_count: 1,
    last_run: null,
  },
];

function show(list: unknown) {
  vi.stubGlobal(
    "fetch",
    vi.fn(async () => ({ ok: true, status: 200, text: async () => JSON.stringify(list) })),
  );
  const client = new QueryClient({ defaultOptions: { queries: { retry: false } } });
  // The route the dialog opens is in, so the page the test looks at is really the one a person reaches.
  render(
    <QueryClientProvider client={client}>
      <MemoryRouter initialEntries={["/workflows"]}>
        <Routes>
          <Route path="/workflows" element={<WorkflowsPage />} />
          <Route path="/workflows/:name/canvas" element={<CanvasProbe />} />
        </Routes>
      </MemoryRouter>
    </QueryClientProvider>,
  );
}

/** Stands in for the canvas, so a test sees what the dialog opened. */
function CanvasProbe() {
  const { state } = useLocation();
  return <div data-testid="canvas-probe">{JSON.stringify(state)}</div>;
}

afterEach(() => vi.unstubAllGlobals());

describe("the workflows list", () => {
  it("says what each workflow is, what starts it, its nodes and its last run", async () => {
    show(LIST);
    expect(await screen.findByRole("heading", { name: "release" })).toBeTruthy();
    expect(screen.getByText("Build, test and publish a release.")).toBeTruthy();
    expect(screen.getByText("Schedule · 0 6 * * 1")).toBeTruthy();
    expect(screen.getByText("3 nodes")).toBeTruthy();
    expect(screen.getByRole("link", { name: /Failed/ })).toBeTruthy();

    expect(screen.getByRole("heading", { name: "triage" })).toBeTruthy();
    expect(screen.getByText("Manual")).toBeTruthy();
    expect(screen.getByText("Never run")).toBeTruthy();
  });

  it("opens the definition by its name, escaping the name in the address", async () => {
    show([{ ...LIST[0], name: "night build" }]);
    const link = await screen.findByRole("link", { name: "night build" });
    expect(link.getAttribute("href")).toBe("/workflows/night%20build");
  });

  it("says there is no workflow rather than drawing an empty list", async () => {
    show([]);
    expect(await screen.findByText("No workflow yet")).toBeTruthy();
  });

  it("offers a new workflow from the header and from the empty state", async () => {
    show(LIST);
    fireEvent.click(await screen.findByRole("button", { name: "New workflow" }));
    expect(await screen.findByRole("dialog", { name: "New workflow" })).toBeTruthy();

    fireEvent.click(screen.getByRole("button", { name: "Cancel" }));
    show([]);
    await screen.findByText("No workflow yet");
    // The same call to action appears in the header and in the empty state; the one outside the header is the empty
    // state's.
    const emptyCta = screen
      .getAllByRole("button", { name: "New workflow" })
      .find((button) => button.closest("header") === null);
    if (!emptyCta) {
      throw new Error("the empty state has no New workflow button");
    }
    fireEvent.click(emptyCta);
    expect(await screen.findByRole("dialog", { name: "New workflow" })).toBeTruthy();
  });

  it("refuses a name the file rules do not allow, and one that is taken", async () => {
    show(LIST);
    fireEvent.click(await screen.findByRole("button", { name: "New workflow" }));
    const open = await screen.findByTestId("new-workflow-create");
    const name = screen.getByLabelText("Name");

    fireEvent.change(name, { target: { value: "night build" } });
    expect(
      screen.getByText("A workflow's name may hold only letters, digits, - and _."),
    ).toBeTruthy();
    expect(open.hasAttribute("disabled")).toBe(true);

    fireEvent.change(name, { target: { value: "release" } });
    expect(await screen.findByText("A workflow with this name exists already.")).toBeTruthy();
    expect(open.hasAttribute("disabled")).toBe(true);
  });

  it("opens the canvas with a blank draft under the name the dialog confirmed", async () => {
    show(LIST);
    fireEvent.click(await screen.findByRole("button", { name: "New workflow" }));
    await screen.findByRole("dialog", { name: "New workflow" });
    fireEvent.change(screen.getByLabelText("Name"), { target: { value: "nightly" } });
    fireEvent.change(screen.getByLabelText("Description"), {
      target: { value: "Build the change every night." },
    });
    fireEvent.click(screen.getByTestId("new-workflow-create"));

    const probe = await screen.findByTestId("canvas-probe");
    expect(JSON.parse(probe.textContent ?? "null")).toEqual({
      draft: {
        name: "nightly",
        description: "Build the change every night.",
        trigger: [{ kind: "manual", params: {} }],
        nodes: [],
        edges: [],
      },
    });
  });
});
