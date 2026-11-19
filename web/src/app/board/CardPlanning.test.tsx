// @vitest-environment jsdom
/**
 * The people, milestones and cycles of a card, set from the card's properties.
 */
import { cleanup, fireEvent, screen, waitFor } from "@testing-library/react";
import { afterEach, describe, expect, it, vi } from "vitest";
import { card, properties, server } from "./cardPropertiesFixtures";

afterEach(() => {
  cleanup();
  vi.unstubAllGlobals();
});

describe("the assignees of a card", () => {
  it("offers the people who are not disabled and adds the one person chosen", async () => {
    const sent = server();
    properties(card(5, { assignees: ["owner"] }));
    fireEvent.keyDown(screen.getByRole("button", { name: "Assignee @owner" }), { key: "Enter" });
    expect(
      (await screen.findByRole("menuitemcheckbox", { name: "Owner (@owner)" })).getAttribute(
        "aria-checked",
      ),
    ).toBe("true");
    expect(
      screen.getByRole("menuitemcheckbox", { name: "Lea (@lea)" }).getAttribute("aria-checked"),
    ).toBe("false");
    expect(screen.queryByRole("menuitemcheckbox", { name: /tom/ })).toBeNull();
    fireEvent.click(screen.getByRole("menuitemcheckbox", { name: "Lea (@lea)" }));
    await waitFor(() => expect(sent).toHaveLength(1));
    expect(sent[0]).toEqual(["PUT", "/v1/cards/5/assignees/lea", undefined]);
  });

  it("takes a person off, and keeps showing one who was disabled after being assigned", async () => {
    const sent = server();
    properties(card(5, { assignees: ["lea", "tom"] }));
    fireEvent.keyDown(screen.getByRole("button", { name: /^Assignee @lea/ }), {
      key: "Enter",
    });
    // The people are read once the menu is open, so wait for a name only they give.
    const lea = await screen.findByRole("menuitemcheckbox", { name: "Lea (@lea)" });
    expect(
      screen.getByRole("menuitemcheckbox", { name: "@tom" }).getAttribute("aria-checked"),
    ).toBe("true");
    fireEvent.click(lea);
    await waitFor(() => expect(sent).toHaveLength(1));
    expect(sent[0]).toEqual(["DELETE", "/v1/cards/5/assignees/lea", undefined]);
  });

  it("says no one while the card is given to nobody", () => {
    server();
    properties(card(5));
    expect(screen.getByRole("button", { name: "Assignee No one" })).toBeTruthy();
  });
});

describe("the milestones of a card", () => {
  const launch = {
    id: 1,
    name: "Launch",
    target_date: "2027-01-31",
    description: "",
    created_at: 0,
  };
  const report = {
    id: 2,
    name: "Report",
    target_date: "2027-03-01",
    description: "",
    created_at: 0,
  };
  const view = (m: typeof launch) => ({ ...m, cards: 0, done: 0, projects: [] });

  it("says none while the card is in no milestone", async () => {
    server();
    properties(card(5));
    expect(await screen.findByRole("button", { name: "Milestone None" })).toBeTruthy();
  });

  it("shows the milestones the card is in and saves the whole set when another is added", async () => {
    const sent = server({
      "GET /v1/cards/5/milestones": { status: 200, body: [launch] },
      "GET /v1/milestones": { status: 200, body: [view(launch), view(report)] },
    });
    properties(card(5));
    const property = await screen.findByRole("button", { name: "Milestone Launch" });
    fireEvent.keyDown(property, { key: "Enter" });
    expect(
      (await screen.findByRole("menuitemcheckbox", { name: "Launch" })).getAttribute(
        "aria-checked",
      ),
    ).toBe("true");
    fireEvent.click(await screen.findByRole("menuitemcheckbox", { name: "Report" }));
    await waitFor(() => expect(sent).toHaveLength(1));
    expect(sent[0]).toEqual(["PUT", "/v1/cards/5/milestones", { milestones: [1, 2] }]);
  });
});

describe("the cycle of a card", () => {
  const cycle = (id: number, name: string, closed_at: number | null = null) => ({
    id,
    project_id: 1,
    name,
    starts_on: "2026-11-02",
    ends_on: "2026-11-15",
    closed_at,
    created_at: 0,
    cards: 0,
    done: 0,
    points: 0,
    points_done: 0,
  });

  it("says none while the card is in no cycle", async () => {
    server();
    properties(card(5));
    expect(await screen.findByRole("button", { name: "Cycle None" })).toBeTruthy();
  });

  it("offers the open cycles of the card's project and saves the one chosen", async () => {
    const sent = server({
      "GET /v1/cards/5/cycle": { status: 200, body: cycle(1, "Cycle 1") },
      "GET /v1/projects/1/cycles": {
        status: 200,
        body: [cycle(2, "Cycle 2"), cycle(1, "Cycle 1"), cycle(3, "Old", 9)],
      },
    });
    properties(card(5));
    fireEvent.keyDown(await screen.findByRole("button", { name: "Cycle Cycle 1" }), {
      key: "Enter",
    });
    expect(await screen.findByRole("menuitemradio", { name: "Cycle 2" })).toBeTruthy();
    expect(screen.queryByRole("menuitemradio", { name: "Old" })).toBeNull();
    fireEvent.click(screen.getByRole("menuitemradio", { name: "Cycle 2" }));
    await waitFor(() => expect(sent).toHaveLength(1));
    expect(sent[0]).toEqual(["PUT", "/v1/cards/5/cycle", { cycle: 2 }]);
  });
});
