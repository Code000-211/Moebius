// @vitest-environment jsdom
import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { cleanup, fireEvent, render, screen, waitFor } from "@testing-library/react";
import { MemoryRouter } from "react-router";
import { afterEach, describe, expect, it, vi } from "vitest";
import { keys } from "../../api/keys";
import type { IntakeRequest } from "../../api/types/intake";
import { IntakePage } from "./IntakePage";

afterEach(() => {
  cleanup();
  vi.unstubAllGlobals();
});

const project = (intake: boolean) => ({
  id: 1,
  name: "Shop",
  key: "SHOP",
  created_at: 0,
  repositories: [],
  members: [],
  open_cards: 0,
  files: null,
  intake,
});

const drafted: IntakeRequest = {
  id: 5,
  project_id: 1,
  requester: "kim",
  text: "Please add a dark mode.",
  title: "Add a dark mode",
  body: "A theme switch.",
  status: "drafted",
  run_id: 3,
  card_id: null,
  reason: "",
  created_at: 0,
  decided_at: null,
};

/** A server with one project and `requests`; what is sent with POST or PUT is recorded as `[method path, body]`. */
function server(intake: boolean, requests: IntakeRequest[]) {
  const sent: Array<[string, unknown]> = [];
  vi.stubGlobal(
    "fetch",
    vi.fn(async (url: string, init?: { method?: string; body?: string }) => {
      const reply = (body: unknown) => ({
        ok: true,
        status: 200,
        text: async () => JSON.stringify(body),
      });
      if (init?.method === "POST" || init?.method === "PUT") {
        sent.push([`${init.method} ${url}`, JSON.parse(init.body ?? "{}")]);
        return reply(drafted);
      }
      if (url === "/v1/snapshot") {
        return reply({
          boards: [{ repository: { id: 8, name: "shop" }, cards: [{ id: 77, number: 1 }] }],
        });
      }
      return reply(url === "/v1/projects" ? [project(intake)] : requests);
    }),
  );
  return sent;
}

function show(role: string) {
  const client = new QueryClient({ defaultOptions: { queries: { retry: false } } });
  client.setQueryData(keys.session, { signed_in: true, person: { handle: "me", role } });
  render(
    <QueryClientProvider client={client}>
      <MemoryRouter>
        <IntakePage />
      </MemoryRouter>
    </QueryClientProvider>,
  );
}

describe("the Intake page", () => {
  it("has no form for a project without intake, and lets an owner turn it on", async () => {
    const sent = server(false, []);
    show("owner");
    await screen.findByText("Shop takes no requests");
    expect(screen.queryByRole("form", { name: "Ask for something" })).toBeNull();
    fireEvent.click(screen.getByRole("button", { name: "Turn on intake" }));
    await waitFor(() => expect(sent).toEqual([["PUT /v1/projects/1/intake", { enabled: true }]]));
  });

  it("shows a viewer the form and their own requests, without a way to decide", async () => {
    const sent = server(true, [drafted]);
    show("viewer");
    const form = await screen.findByRole("form", { name: "Ask for something" });
    expect(form).toBeTruthy();
    await screen.findByText("Please add a dark mode.");
    expect(screen.queryByRole("button", { name: "Accept" })).toBeNull();
    expect(screen.queryByRole("button", { name: "Decline" })).toBeNull();
    fireEvent.change(screen.getByLabelText("What do you need?"), {
      target: { value: "Export the report" },
    });
    fireEvent.click(screen.getByRole("button", { name: "Send request" }));
    await waitFor(() =>
      expect(sent).toEqual([["POST /v1/projects/1/requests", { text: "Export the report" }]]),
    );
  });

  it("shows the draft of a request that was being grilled when the page was open", async () => {
    const grilling: IntakeRequest = { ...drafted, status: "grilling", title: "", body: "" };
    const requests = [grilling];
    server(true, requests);
    const client = new QueryClient({ defaultOptions: { queries: { retry: false } } });
    client.setQueryData(keys.session, {
      signed_in: true,
      person: { handle: "me", role: "member" },
    });
    render(
      <QueryClientProvider client={client}>
        <MemoryRouter>
          <IntakePage />
        </MemoryRouter>
      </QueryClientProvider>,
    );
    await screen.findByText("Please add a dark mode.");
    expect(screen.queryByLabelText("Draft issue")).toBeNull();
    requests[0] = drafted;
    await client.invalidateQueries({ queryKey: keys.requests(1) });
    const title = (await screen.findByLabelText("Draft issue")) as HTMLInputElement;
    expect(title.value).toBe("Add a dark mode");
  });

  it("lets a member edit and accept a draft, and decline only with a reason", async () => {
    const sent = server(true, [drafted]);
    show("member");
    await screen.findByDisplayValue("Add a dark mode");
    fireEvent.change(screen.getByLabelText("Draft issue"), { target: { value: "Dark theme" } });
    fireEvent.click(screen.getByRole("button", { name: "Accept" }));
    await waitFor(() =>
      expect(sent).toEqual([
        ["POST /v1/requests/5/accept", { title: "Dark theme", body: "A theme switch." }],
      ]),
    );
    const decline = screen.getByRole("button", { name: "Decline" }) as HTMLButtonElement;
    expect(decline.disabled).toBe(true);
    fireEvent.change(screen.getByLabelText("Reason for declining"), {
      target: { value: "Out of scope" },
    });
    expect(decline.disabled).toBe(false);
    fireEvent.click(decline);
    await waitFor(() =>
      expect(sent[1]).toEqual(["POST /v1/requests/5/decline", { reason: "Out of scope" }]),
    );
  });

  it("links an accepted request to the card it made", async () => {
    server(true, [{ ...drafted, status: "accepted", card_id: 77 }]);
    show("member");
    const link = await screen.findByRole("link", { name: "Open the card" });
    expect(link.getAttribute("href")).toBe("/board/8?card=77");
  });
});
