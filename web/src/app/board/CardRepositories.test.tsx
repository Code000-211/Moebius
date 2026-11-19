// @vitest-environment jsdom
/**
 * The repositories of a card and its changes over several repositories, as the card's properties show and set them.
 */
import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { cleanup, fireEvent, render, screen, waitFor, within } from "@testing-library/react";
import { MemoryRouter } from "react-router";
import { afterEach, describe, expect, it, vi } from "vitest";
import { CardProperties } from "./CardProperties";
import { board, card, properties, region, server } from "./cardPropertiesFixtures";

afterEach(() => {
  cleanup();
  vi.unstubAllGlobals();
});

describe("the repositories of a card", () => {
  const PROJECT = {
    id: 1,
    repositories: [
      { id: 1, name: "demo", home: true },
      { id: 2, name: "web", home: false },
      { id: 3, name: "docs", home: false },
    ],
    files: null,
  };
  const projects = { "GET /v1/projects": { status: 200, body: [PROJECT] } };

  it("lists the repositories the card works in, its own first and fixed", async () => {
    server(projects);
    properties(card(5, { repositories: ["demo", "web"] }));
    const button = screen.getByRole("button", { name: /^Repositories/ });
    expect(button.textContent).toContain("demo (own)");
    expect(button.textContent).toContain("web");
    fireEvent.keyDown(button, { key: "Enter" });
    const own = await screen.findByRole("menuitemcheckbox", { name: "demo" });
    expect(own.getAttribute("aria-checked")).toBe("true");
    expect(own.getAttribute("aria-disabled")).toBe("true");
    expect(screen.getByRole("menuitemcheckbox", { name: "web" }).getAttribute("aria-checked")).toBe(
      "true",
    );
    // The project's other repositories are read once the menu is open.
    expect(
      (await screen.findByRole("menuitemcheckbox", { name: "docs" })).getAttribute("aria-checked"),
    ).toBe("false");
  });

  it("adds another repository of the project after the ones named, as the repositories besides its own", async () => {
    const sent = server(projects);
    properties(card(5, { repositories: ["demo", "web"] }));
    fireEvent.keyDown(screen.getByRole("button", { name: /^Repositories/ }), { key: "Enter" });
    fireEvent.click(await screen.findByRole("menuitemcheckbox", { name: "docs" }));
    await waitFor(() => expect(sent).toHaveLength(1));
    expect(sent[0]).toEqual(["PUT", "/v1/cards/5/repositories", { repositories: ["web", "docs"] }]);
  });

  it("says why the server refuses to take off a repository with an open change", async () => {
    const reason =
      "the card has an open change in web: it cannot leave the card while the change is open";
    const sent = server({
      ...projects,
      "PUT /v1/cards/5/repositories": { status: 422, body: reason },
    });
    properties(card(5, { repositories: ["demo", "web"] }));
    fireEvent.keyDown(screen.getByRole("button", { name: /^Repositories/ }), { key: "Enter" });
    fireEvent.click(await screen.findByRole("menuitemcheckbox", { name: "web" }));
    await waitFor(() => expect(sent).toHaveLength(1));
    expect(sent[0]).toEqual(["PUT", "/v1/cards/5/repositories", { repositories: [] }]);
    expect(await screen.findByText(reason)).toBeTruthy();
  });

  it("shows only its own repository for a card of a server that does not send the list", () => {
    server(projects);
    properties(card(5));
    expect(screen.getByRole("button", { name: /^Repositories/ }).textContent).toContain(
      "demo (own)",
    );
  });
});

describe("the changes of a card over several repositories", () => {
  const change = (id: number, repository: string, checks: "success" | "failure") => ({
    id,
    repository,
    state: "open",
    checks,
    review_verdict: null,
  });

  it("shows the checks of each change with its repository", () => {
    server();
    const own = change(1, "demo", "success");
    properties(card(5, { change: own, changes: [own, change(2, "web", "failure")] }));
    const facts = within(region(5));
    expect(facts.getByText("Checks in demo").parentElement?.textContent).toContain("Green");
    expect(facts.getByText("Checks in web").parentElement?.textContent).toContain("Failed");
  });

  it("names no repository for the one change of a card over one repository", () => {
    server();
    const own = change(1, "demo", "success");
    properties(card(5, { change: own, changes: [own] }));
    expect(within(region(5)).getByText("Checks").parentElement?.textContent).toContain("Green");
  });

  it("says what a stopped merge left merged and what is still open", () => {
    server();
    const stopped = {
      id: 9,
      kind: "merge_stopped",
      card_id: 5,
      title: "The merge stopped",
      data: {
        change: 2,
        repository: "web",
        reason: "the change no longer merges",
        merged: ["demo"],
        open: ["web", "docs"],
      },
      resolved_at: null,
      at: 0,
      assignees: [],
    };
    const c = card(5);
    const client = new QueryClient({ defaultOptions: { queries: { retry: false } } });
    render(
      <QueryClientProvider client={client}>
        <MemoryRouter>
          <CardProperties
            board={board([c])}
            card={c}
            item={undefined}
            stopped={stopped}
            onOpenCard={vi.fn()}
          />
        </MemoryRouter>
      </QueryClientProvider>,
    );
    const facts = within(region(5));
    expect(facts.getByText("Merged").parentElement?.textContent).toContain("demo");
    const open = facts.getByText("Still open").parentElement?.textContent ?? "";
    expect(open).toContain("web, docs");
    expect(open).toContain("The merge in web stopped: the change no longer merges.");
  });
});
