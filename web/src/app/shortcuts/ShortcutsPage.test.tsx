/**
 * The Keyboard shortcuts page lists every shortcut of `shell/palette/shortcuts.ts`, each with its keys.
 */
import { cleanup, render, screen, within } from "@testing-library/react";
import { MemoryRouter } from "react-router";
import { afterEach, describe, expect, it } from "vitest";
import { CARD_KEYS, SHORTCUT_GROUPS } from "../shell/palette/shortcuts";
import { ShortcutsPage } from "./ShortcutsPage";

afterEach(cleanup);

describe("the shortcuts page", () => {
  it("lists every group and every shortcut of the list", () => {
    render(
      <MemoryRouter>
        <ShortcutsPage />
      </MemoryRouter>,
    );
    screen.getByRole("heading", { name: "Keyboard shortcuts" });
    for (const group of SHORTCUT_GROUPS) {
      const section = screen.getByRole("region", { name: group.title });
      for (const shortcut of group.shortcuts) within(section).getByText(shortcut.label);
    }
  });

  it("has a row for the key of every command of a card", () => {
    const rows = SHORTCUT_GROUPS.flatMap((g) => g.shortcuts.flatMap((s) => s.keys));
    for (const key of Object.values(CARD_KEYS)) expect(rows).toContain(key.toUpperCase());
  });
});
