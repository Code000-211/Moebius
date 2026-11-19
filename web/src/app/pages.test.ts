import { describe, expect, it } from "vitest";
import { pageByPath, SETTINGS_PAGES, shownTo } from "./pages";

describe("the pages only an owner is shown", () => {
  it("leaves People out for a member and a viewer and keeps a person's own tokens for everyone", () => {
    const people = pageByPath("people");
    const tokens = pageByPath("tokens");
    expect(people && tokens).toBeTruthy();
    if (!people || !tokens) return;
    expect(shownTo(people, "owner")).toBe(true);
    expect(shownTo(people, "member")).toBe(false);
    expect(shownTo(people, "viewer")).toBe(false);
    for (const role of ["owner", "member", "viewer"]) expect(shownTo(tokens, role)).toBe(true);
    expect(SETTINGS_PAGES).toContain(people);
    expect(SETTINGS_PAGES).toContain(tokens);
  });
});
