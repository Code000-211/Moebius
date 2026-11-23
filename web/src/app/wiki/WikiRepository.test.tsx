// @vitest-environment jsdom
import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { cleanup, fireEvent, render, screen, waitFor } from "@testing-library/react";
import { afterEach, describe, expect, it, vi } from "vitest";
import { keys } from "../../api/keys";
import type { WikiStatus } from "../../api/types/wiki";
import { ToastProvider } from "../../ui/page/Toast";
import { WikiRepositoryNotice } from "./WikiRepository";

afterEach(() => {
  cleanup();
  vi.unstubAllGlobals();
});

const LOCAL: WikiStatus = { state: "local", url: null, problem: null };

function show(role: string, status: WikiStatus, connect?: (body: unknown) => Response) {
  const calls: Array<{ method: string; url: string; body: unknown }> = [];
  vi.stubGlobal(
    "fetch",
    vi.fn(async (url: string, init?: RequestInit) => {
      const method = init?.method ?? "GET";
      const body = init?.body ? JSON.parse(String(init.body)) : undefined;
      calls.push({ method, url, body });
      if (method === "PUT" && url.endsWith("/v1/wiki/repository") && connect) {
        return connect(body);
      }
      return { ok: true, status: 200, text: async () => JSON.stringify(status) };
    }),
  );
  const client = new QueryClient({ defaultOptions: { queries: { retry: false } } });
  client.setQueryData(keys.session, { signed_in: true, person: { handle: "me", role } });
  render(
    <QueryClientProvider client={client}>
      <ToastProvider>
        <WikiRepositoryNotice />
      </ToastProvider>
    </QueryClientProvider>,
  );
  return calls;
}

describe("the notice about where the wiki is kept", () => {
  it("tells an owner the pages are on this server only and offers a repository", async () => {
    show("owner", LOCAL);
    expect(await screen.findByText(/kept on this server only/)).toBeTruthy();
    expect(screen.getByRole("button", { name: "Use a git repository…" })).toBeTruthy();
  });

  it("is not shown to a member or a viewer, nor for a wiki that is in a repository", async () => {
    for (const [role, status] of [
      ["member", LOCAL],
      ["viewer", LOCAL],
      ["owner", { state: "remote", url: "https://example.com/w.git", problem: null }],
    ] as const) {
      const calls = show(role, status);
      await waitFor(() => expect(calls.length).toBeGreaterThan(0));
      await new Promise((done) => setTimeout(done, 20));
      expect(screen.queryByText(/kept on this server only/)).toBeNull();
      cleanup();
    }
  });

  it("says why a wiki could not be opened", async () => {
    show("owner", { state: "unavailable", url: null, problem: "cannot clone the wiki: no route" });
    expect(
      await screen.findByText(/could not be opened: cannot clone the wiki: no route/),
    ).toBeTruthy();
  });

  it("sends the address and closes when the server connects it", async () => {
    const calls = show(
      "owner",
      LOCAL,
      () =>
        ({
          ok: true,
          status: 200,
          text: async () =>
            JSON.stringify({ state: "remote", url: "https://example.com/w.git", problem: null }),
        }) as Response,
    );
    fireEvent.click(await screen.findByRole("button", { name: "Use a git repository…" }));
    fireEvent.change(screen.getByLabelText("Repository address"), {
      target: { value: "https://example.com/w.git" },
    });
    fireEvent.click(screen.getByRole("button", { name: "Connect" }));
    await waitFor(() => expect(screen.queryByRole("dialog")).toBeNull());
    const put = calls.find((call) => call.method === "PUT");
    expect(put?.body).toEqual({ url: "https://example.com/w.git" });
  });

  it("shows the refusal of the server in the dialog and keeps it open", async () => {
    show(
      "owner",
      LOCAL,
      () =>
        ({
          ok: false,
          status: 409,
          text: async () => "both this wiki and the repository have pages",
        }) as Response,
    );
    fireEvent.click(await screen.findByRole("button", { name: "Use a git repository…" }));
    fireEvent.change(screen.getByLabelText("Repository address"), {
      target: { value: "https://example.com/w.git" },
    });
    fireEvent.click(screen.getByRole("button", { name: "Connect" }));
    expect(await screen.findByText(/both this wiki and the repository have pages/)).toBeTruthy();
    expect(screen.getByRole("dialog")).toBeTruthy();
  });
});
