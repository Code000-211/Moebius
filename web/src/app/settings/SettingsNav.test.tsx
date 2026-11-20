import { fireEvent, render, screen } from "@testing-library/react";
import { MemoryRouter } from "react-router";
import { describe, expect, it } from "vitest";
import { SettingsNav } from "./SettingsNav";
import { shownEntries } from "./settingsEntries";

const show = () =>
  render(
    <MemoryRouter initialEntries={["/models"]}>
      <SettingsNav entries={shownEntries("owner")} />
    </MemoryRouter>,
  );

describe("SettingsNav", () => {
  it("groups the entries and marks the one that is open", () => {
    show();
    expect(screen.getByText("Automation")).toBeTruthy();
    expect(screen.getByRole("link", { name: "Models" }).getAttribute("aria-current")).toBe("page");
    expect(screen.queryByRole("link", { name: "Profile" })).toBeNull();
  });

  it("narrows as one types and says so in one line when nothing matches", () => {
    show();
    fireEvent.change(screen.getByRole("searchbox", { name: "Search settings" }), {
      target: { value: "ret" },
    });
    expect(screen.getAllByRole("link").map((l) => l.textContent)).toEqual(["Retro"]);
    fireEvent.change(screen.getByRole("searchbox", { name: "Search settings" }), {
      target: { value: "zzz" },
    });
    expect(screen.queryAllByRole("link")).toHaveLength(0);
    expect(screen.getByText("No settings match.")).toBeTruthy();
  });
});
