/**
 * The Automations page: the chosen automation is in the address and is edited in the editor window, which saves the
 * whole automation.
 */
import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { fireEvent, render, screen, waitFor } from "@testing-library/react";
import { MemoryRouter, useLocation } from "react-router";
import { afterEach, describe, expect, it, vi } from "vitest";
import { AutomationsPage } from "./AutomationsPage";

const AUTOMATION = {
  id: 3,
  name: "Morning triage",
  schedule: "0 8 * * *",
  agent: "Moebius",
  prompt: "Triage the issues.",
  repository_id: null,
  harness_repository: false,
  enabled: true,
  next_firing: null,
  last_firing: null,
};

function Where() {
  return <span data-testid="where">{useLocation().search}</span>;
}

const asked: string[] = [];

function show(initial: string, gone = false) {
  const sent: Array<{ url: string; method: string; body: unknown }> = [];
  asked.length = 0;
  vi.stubGlobal(
    "fetch",
    vi.fn(async (url: string, init?: RequestInit) => {
      const method = init?.method ?? "GET";
      asked.push(`${method} ${url}`);
      if (method !== "GET")
        sent.push({ url, method, body: JSON.parse(String(init?.body ?? "null")) });
      if (gone && url.includes("/firings")) {
        return { ok: false, status: 404, text: async () => JSON.stringify({ error: "not found" }) };
      }
      const body = url.endsWith("/v1/automations")
        ? [AUTOMATION]
        : url.includes("/firings")
          ? []
          : url.includes("/v1/agents")
            ? [{ name: "Moebius" }, { name: "Coder" }]
            : url.includes("/v1/automations/")
              ? AUTOMATION
              : { boards: [] };
      return { ok: true, status: 200, text: async () => JSON.stringify(body) };
    }),
  );
  render(
    <QueryClientProvider
      client={new QueryClient({ defaultOptions: { queries: { retry: false } } })}
    >
      <MemoryRouter initialEntries={[initial]}>
        <AutomationsPage />
        <Where />
      </MemoryRouter>
    </QueryClientProvider>,
  );
  return sent;
}

afterEach(() => vi.unstubAllGlobals());

describe("the automations list and its editor window", () => {
  it("puts the chosen automation in the address and opens it in the editor window", async () => {
    show("/automations");
    fireEvent.click(await screen.findByTestId("automation-row"));
    expect(screen.getByTestId("where").textContent).toBe("?automation=3");
    expect(await screen.findByTestId("editor-window")).toBeTruthy();
    expect(((await screen.findByLabelText("Name")) as HTMLInputElement).value).toBe(
      "Morning triage",
    );
  });

  it("saves the whole automation with Save, and leaves the address", async () => {
    const sent = show("/automations?automation=3");
    const field = await screen.findByLabelText("Name");
    fireEvent.change(field, { target: { value: "Evening triage" } });
    fireEvent.click(screen.getByRole("button", { name: "Save" }));
    await waitFor(() => expect(sent).toHaveLength(1));
    expect(sent[0]).toEqual({
      url: "/v1/automations/3",
      method: "PUT",
      body: {
        name: "Evening triage",
        schedule: "0 8 * * *",
        agent: "Moebius",
        prompt: "Triage the issues.",
        repository: null,
        harness: false,
        enabled: true,
      },
    });
    await waitFor(() => expect(screen.getByTestId("where").textContent).toBe(""));
  });

  it("keeps a new automation that is missing its name in the window, and sends nothing", async () => {
    const sent = show("/automations");
    fireEvent.click(await screen.findByRole("button", { name: "New automation" }));
    fireEvent.change(await screen.findByLabelText("Prompt"), { target: { value: "Say hi." } });
    fireEvent.click(await screen.findByRole("button", { name: "Save" }));
    expect(await screen.findByText("Give the automation a name.")).toBeTruthy();
    expect(screen.getByTestId("editor-window")).toBeTruthy();
    expect(sent).toEqual([]);
  });

  it("does not ask for the past runs of an automation it just deleted", async () => {
    show("/automations?automation=3");
    fireEvent.click(await screen.findByRole("button", { name: "Delete automation" }));
    asked.length = 0;
    fireEvent.click(
      await screen.findByRole("button", { name: "Delete automation", hidden: false }),
    );
    await waitFor(() =>
      expect(asked.some((a) => a.startsWith("DELETE /v1/automations/3"))).toBe(true),
    );
    await new Promise((r) => setTimeout(r, 100));
    expect(asked.filter((a) => a.includes("/firings"))).toEqual([]);
  });
});
