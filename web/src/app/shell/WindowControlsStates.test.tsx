// @vitest-environment jsdom
/**
 * The window's controls in the desktop app on the Inbox in every state it can be in: while its snapshot is read, when
 * that fails and once it is there. The shell mounts them whatever its page draws, so a person is never left with a
 * window that cannot be minimised, maximised or closed (mocked desktop bridge).
 */
import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { cleanup, render, screen, waitFor } from "@testing-library/react";
import { MemoryRouter, Route, Routes } from "react-router";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { fakeBridge } from "../../desktop/fakeBridge";
import { mockViewport } from "../../ui/page/virtualListTest";
import { App } from "../App";
import { InboxPage } from "../inbox/InboxPage";

let restore: () => void;
beforeEach(() => {
  restore = mockViewport();
  window.moebiusDesktop = fakeBridge();
});
afterEach(() => {
  cleanup();
  restore();
  vi.unstubAllGlobals();
  delete window.moebiusDesktop;
});

const SNAPSHOT = { boards: [], harness_changes: [], inbox: [] };

/** The Inbox of a signed-in person, whose snapshot the server answers by `snapshot`. */
function open(snapshot: () => Promise<Response>) {
  vi.stubGlobal(
    "fetch",
    vi.fn(async (input: RequestInfo | URL) => {
      const path = new URL(String(input), "http://localhost").pathname;
      if (path === "/v1/session") return Response.json({ signed_in: true });
      if (path === "/v1/snapshot") return snapshot();
      if (path === "/v1/notices" || path === "/v1/projects") return Response.json([]);
      return Response.json({});
    }),
  );
  const queryClient = new QueryClient({ defaultOptions: { queries: { retry: false } } });
  render(
    <QueryClientProvider client={queryClient}>
      <MemoryRouter initialEntries={["/inbox"]}>
        <Routes>
          <Route element={<App />}>
            <Route path="inbox" element={<InboxPage />} />
          </Route>
        </Routes>
      </MemoryRouter>
    </QueryClientProvider>,
  );
}

function expectControls() {
  const controls = screen.getByTestId("window-controls");
  expect([...controls.querySelectorAll("button")].map((b) => b.getAttribute("aria-label"))).toEqual(
    ["Minimize", "Maximize", "Close"],
  );
}

describe("the window's controls on the Inbox", () => {
  it("are there while the snapshot is being read", async () => {
    open(() => new Promise<Response>(() => {}));
    await screen.findByRole("heading", { name: "Inbox" });
    expectControls();
  });

  it("are there when the snapshot cannot be read", async () => {
    open(async () => new Response("no", { status: 500 }));
    await screen.findByRole("heading", { name: "Inbox" });
    await screen.findByRole("button", { name: "Try again" });
    expectControls();
  });

  it("are there once the Inbox is loaded", async () => {
    open(async () => Response.json(SNAPSHOT));
    await screen.findByRole("heading", { name: "Inbox" });
    await waitFor(() => expect(screen.queryByText(/Reading the inbox/)).toBeNull());
    expectControls();
  });
});
