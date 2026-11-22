// @vitest-environment jsdom
import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { cleanup, fireEvent, render, screen } from "@testing-library/react";
import { MemoryRouter, Route, Routes, useLocation } from "react-router";
import { afterEach, describe, expect, it, vi } from "vitest";
import { keys } from "../../api/keys";
import type { Project } from "../../api/types/projects";
import { ProjectHeader } from "./ProjectHeader";

afterEach(cleanup);

const project = (intake: boolean): Project => ({
  id: 7,
  name: "Platform",
  key: "PLT",
  created_at: 1,
  private: false,
  repositories: [{ id: 1, name: "core", home: true }],
  members: [],
  open_cards: 0,
  files: null,
  kinds: [],
  intake,
  member: true,
});

function Where() {
  const at = useLocation();
  return <p data-testid="where">{at.pathname}</p>;
}

function show(role: string, intake: boolean, onTerminal = vi.fn()) {
  const client = new QueryClient({ defaultOptions: { queries: { retry: false } } });
  client.setQueryData(keys.session, { person: { role } });
  client.setQueryData(keys.requests(7), [
    { id: 1, status: "drafted" },
    { id: 2, status: "drafted" },
    { id: 3, status: "accepted" },
  ]);
  render(
    <QueryClientProvider client={client}>
      <MemoryRouter initialEntries={["/projects/7"]}>
        <Where />
        <Routes>
          <Route
            path="*"
            element={
              <ProjectHeader project={project(intake)} tab="board" onTerminal={onTerminal} />
            }
          />
        </Routes>
      </MemoryRouter>
    </QueryClientProvider>,
  );
  return onTerminal;
}

async function openMenu() {
  const trigger = screen.getByRole("button", { name: "More project actions" });
  trigger.focus();
  fireEvent.keyDown(trigger, { key: "Enter" });
}

describe("the project header", () => {
  it("names the project and its four tabs, and links the gear to the settings", () => {
    show("member", false);
    expect(screen.getByRole("heading", { name: "Platform" })).toBeTruthy();
    const names = screen.getAllByRole("tab").map((each) => each.textContent);
    expect(names).toEqual(["Board", "Workflows", "Cycles", "Milestones"]);
    expect(screen.getByRole("link", { name: "Project settings" }).getAttribute("href")).toBe(
      "/projects/7/settings",
    );
  });

  it("puts the tab in the address", () => {
    show("member", false);
    fireEvent.click(screen.getByRole("tab", { name: "Cycles" }));
    expect(screen.getByTestId("where").textContent).toBe("/projects/7/cycles");
    fireEvent.click(screen.getByRole("tab", { name: "Board" }));
    expect(screen.getByTestId("where").textContent).toBe("/projects/7");
  });

  it("shows the Intake tab with the requests waiting only while intake is on", async () => {
    show("member", true);
    expect((await screen.findByRole("tab", { name: /^Intake/ })).textContent).toBe("Intake2");
    cleanup();
    show("member", false);
    expect(screen.queryByRole("tab", { name: /Intake/ })).toBeNull();
  });

  it("offers Import from Jira to an owner, with Terminal", async () => {
    const onTerminal = show("owner", false);
    await openMenu();
    expect(await screen.findByRole("menuitem", { name: "Import from Jira" })).toBeTruthy();
    fireEvent.click(screen.getByRole("menuitem", { name: "Terminal" }));
    expect(onTerminal).toHaveBeenCalledTimes(1);
  });

  it("leaves Import from Jira out for a member", async () => {
    show("member", false);
    await openMenu();
    expect(await screen.findByRole("menuitem", { name: "Terminal" })).toBeTruthy();
    expect(screen.queryByRole("menuitem", { name: "Import from Jira" })).toBeNull();
  });
});
