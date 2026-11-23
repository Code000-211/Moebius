// @vitest-environment jsdom
import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { cleanup, fireEvent, render, screen, waitFor } from "@testing-library/react";
import { MemoryRouter } from "react-router";
import { afterEach, describe, expect, it, vi } from "vitest";
import { keys } from "../../api/keys";
import { ProfilePage } from "./ProfilePage";

afterEach(() => {
  cleanup();
  vi.unstubAllGlobals();
});

const lea = { id: 2, handle: "lea", name: "Lea", role: "member", disabled: false, created_at: 1 };

function show(ui: React.ReactNode, seed: Array<[readonly unknown[], unknown]> = [], routes = {}) {
  vi.stubGlobal(
    "fetch",
    vi.fn(async (input: RequestInfo | URL, init?: RequestInit) => {
      const path = new URL(String(input), "http://x").pathname;
      if (init?.method === "DELETE") return new Response(null, { status: 204 });
      return new Response(JSON.stringify((routes as Record<string, unknown>)[path] ?? []), {
        headers: { "content-type": "application/json" },
      });
    }),
  );
  const client = new QueryClient({ defaultOptions: { queries: { retry: false } } });
  for (const [key, data] of seed) client.setQueryData(key, data);
  return {
    client,
    ...render(
      <QueryClientProvider client={client}>
        <MemoryRouter>{ui}</MemoryRouter>
      </QueryClientProvider>,
    ),
  };
}

describe("Profile", () => {
  it("shows the signed-in person and signs out from the page", async () => {
    show(<ProfilePage />, [[keys.session, { signed_in: true, person: lea }]]);
    expect(screen.getByText("Lea")).toBeTruthy();
    expect(screen.getByText("@lea")).toBeTruthy();
    expect(screen.getByText("member")).toBeTruthy();
    screen.getByRole("button", { name: "Sign out" }).click();
    await screen.findByRole("button", { name: "Signing out" });
    const calls = (fetch as unknown as ReturnType<typeof vi.fn>).mock.calls;
    expect(
      calls.some(([url, init]) => String(url).includes("/v1/session") && init?.method === "DELETE"),
    ).toBe(true);
  });

  it("keeps the person's login on GitHub, and on another forge once a repository is on it", async () => {
    const bodies: unknown[] = [];
    vi.stubGlobal(
      "fetch",
      vi.fn(async (input: RequestInfo | URL, init?: RequestInit) => {
        const path = new URL(String(input), "http://x").pathname;
        if (path === "/v1/logins" && init?.method === "PUT") {
          bodies.push(JSON.parse(String(init.body)));
          return json({ github: "lea-gh", gitlab: "", forgejo: "" });
        }
        if (path === "/v1/logins") return json({ github: "", gitlab: "", forgejo: "" });
        if (path === "/v1/snapshot") {
          const repository = (forge: string) => ({
            id: 1,
            name: "r",
            path: "/r",
            forge,
            default_branch: "main",
            github: null,
            trusted: [],
            home_project_id: 1,
          });
          return json({
            boards: [
              {
                repository: repository("github"),
                cards: [],
                round_limit: 3,
                waiting: {},
                changes: [],
              },
            ],
            harness_changes: [],
            inbox: [],
          });
        }
        return json([]);
      }),
    );
    const client = new QueryClient({ defaultOptions: { queries: { retry: false } } });
    client.setQueryData(keys.session, { signed_in: true, person: lea });
    render(
      <QueryClientProvider client={client}>
        <MemoryRouter>
          <ProfilePage />
        </MemoryRouter>
      </QueryClientProvider>,
    );
    const github = await screen.findByLabelText("Your login on GitHub");
    // No repository is on GitLab or Forgejo, so there is nothing to say a login for there.
    expect(screen.queryByLabelText("Your login on GitLab")).toBeNull();
    expect(screen.queryByLabelText("Your login on Forgejo")).toBeNull();
    const save = screen.getByRole("button", { name: "Save logins" }) as HTMLButtonElement;
    expect(save.disabled).toBe(true);
    fireEvent.change(github, { target: { value: "lea-gh" } });
    fireEvent.click(save);
    await waitFor(() => expect(bodies).toEqual([{ github: "lea-gh", gitlab: "", forgejo: "" }]));
  });
});

const json = (body: unknown) =>
  new Response(JSON.stringify(body), { headers: { "content-type": "application/json" } });
