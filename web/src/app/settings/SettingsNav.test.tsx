import { fireEvent, render, screen } from "@testing-library/react";
import { MemoryRouter } from "react-router";
import { describe, expect, it } from "vitest";
import { settingsEntries } from "../nav";
import { SettingsNav } from "./SettingsNav";

const show = () =>
  render(
    <MemoryRouter initialEntries={["/models"]}>
      <SettingsNav entries={settingsEntries("owner")} />
    </MemoryRouter>,
  );

describe("SettingsNav", () => {
  it("lists exactly the nine entries in the groups You and Server, and marks the one that is open", () => {
    show();
    expect(screen.getByText("You")).toBeTruthy();
    expect(screen.getByText("Server")).toBeTruthy();
    expect(screen.queryByText("Automation")).toBeNull();
    expect(screen.queryByText("Agents")).toBeNull();
    expect(screen.getAllByRole("link").map((l) => l.textContent)).toEqual([
      "Profile",
      "Appearance",
      "Keyboard shortcuts",
      "My tokens",
      "General",
      "Models",
      "Plugins",
      "Kinds of issues",
      "Members",
    ]);
    expect(screen.getByRole("link", { name: "Models" }).getAttribute("aria-current")).toBe("page");
    // Each of these is somewhere else now, or gone.
    for (const gone of ["Notifications", "Schedules", "Column scripts", "Repositories", "Runners"])
      expect(screen.queryByRole("link", { name: gone }), gone).toBeNull();
  });

  it("hides the Server group from anyone who is not an owner", () => {
    render(
      <MemoryRouter initialEntries={["/profile"]}>
        <SettingsNav entries={settingsEntries("member")} />
      </MemoryRouter>,
    );
    expect(screen.getAllByRole("link").map((l) => l.textContent)).toEqual([
      "Profile",
      "Appearance",
      "Keyboard shortcuts",
      "My tokens",
    ]);
    expect(screen.queryByText("Server")).toBeNull();
  });

  it("narrows as one types and says so in one line when nothing matches", () => {
    show();
    fireEvent.change(screen.getByRole("searchbox", { name: "Search settings" }), {
      target: { value: "mod" },
    });
    expect(screen.getAllByRole("link").map((l) => l.textContent)).toEqual(["Models"]);
    fireEvent.change(screen.getByRole("searchbox", { name: "Search settings" }), {
      target: { value: "zzz" },
    });
    expect(screen.queryAllByRole("link")).toHaveLength(0);
    expect(screen.getByText("No settings match.")).toBeTruthy();
  });
});
