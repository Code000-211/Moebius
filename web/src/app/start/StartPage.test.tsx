// @vitest-environment jsdom
import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { cleanup, fireEvent, render, screen, waitFor, within } from "@testing-library/react";
import { MemoryRouter } from "react-router";
import { afterEach, describe, expect, it, vi } from "vitest";
import { ToastProvider } from "../../ui/page/Toast";
import { StartPage } from "./StartPage";

afterEach(() => {
  cleanup();
  vi.unstubAllGlobals();
});

function show(refuseSetUp = false) {
  vi.stubGlobal(
    "fetch",
    vi.fn(async (url: string, init?: RequestInit) => {
      if (init?.method === "POST" && refuseSetUp) {
        return {
          ok: false,
          status: 409,
          text: async () => JSON.stringify({ error: "harness/ exists" }),
        };
      }
      const body = url.endsWith("/v1/start")
        ? {
            done: false,
            warnings: [],
            steps: [
              { key: "upstream", title: "Connect a model provider", detail: "none", done: true },
              {
                key: "harness",
                title: "Keep agents in a repository",
                detail: "not set up",
                done: false,
              },
              { key: "coder", title: "Give the Coder a model", detail: "no model", done: false },
            ],
          }
        : {};
      return { ok: true, status: 200, text: async () => JSON.stringify(body) };
    }),
  );
  render(
    <QueryClientProvider
      client={new QueryClient({ defaultOptions: { queries: { retry: false } } })}
    >
      <ToastProvider>
        <MemoryRouter>
          <StartPage />
        </MemoryRouter>
      </ToastProvider>
    </QueryClientProvider>,
  );
}

describe("Get started", () => {
  it("shows each step as a row with a status icon and a button only on the missing ones", async () => {
    show();
    const rows = await screen.findAllByTestId("start-step");
    expect(rows).toHaveLength(3);
    expect(within(rows[0] as HTMLElement).getByTestId("status-done")).toBeTruthy();
    expect(within(rows[1] as HTMLElement).getByTestId("status-notStarted")).toBeTruthy();
    expect(within(rows[0] as HTMLElement).queryByRole("button")).toBeNull();
    expect(within(rows[1] as HTMLElement).getByRole("button", { name: "Set up" })).toBeTruthy();
    expect(within(rows[2] as HTMLElement).getByRole("link", { name: "Open Agents" })).toBeTruthy();
  });

  it("tells a refused set-up by a toast", async () => {
    show(true);
    fireEvent.click(await screen.findByRole("button", { name: "Set up" }));
    await waitFor(() => expect(screen.getByRole("alert").textContent).toContain("harness/ exists"));
  });
});
