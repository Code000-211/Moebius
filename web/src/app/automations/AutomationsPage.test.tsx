/**
 * The Automations page's properties: the chosen automation is in the address, its settings are edited in the right
 * column, and each is saved alone with the rest of the automation as stored.
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

function show(initial: string) {
  const sent: Array<{ url: string; method: string; body: unknown }> = [];
  vi.stubGlobal(
    "fetch",
    vi.fn(async (url: string, init?: RequestInit) => {
      const method = init?.method ?? "GET";
      if (method !== "GET") sent.push({ url, method, body: JSON.parse(String(init?.body)) });
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

describe("the properties of an automation", () => {
  it("puts the chosen automation in the address", async () => {
    show("/automations");
    fireEvent.click(await screen.findByTestId("list-card"));
    expect(screen.getByTestId("where").textContent).toBe("?automation=3");
    expect(
      await screen.findByRole("region", { name: "Properties of Morning triage" }),
    ).toBeTruthy();
  });

  it("saves one property alone, with the rest as stored", async () => {
    const sent = show("/automations?automation=3");
    fireEvent.click(await screen.findByRole("button", { name: /Name/ }));
    const field = await screen.findByLabelText("Name");
    fireEvent.change(field, { target: { value: "Evening triage" } });
    fireEvent.keyDown(field, { key: "Enter" });
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
  });
});
