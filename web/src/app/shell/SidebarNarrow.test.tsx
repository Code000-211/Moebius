/**
 * The sidebar in a narrow window: nothing of it while closed, a solid overlay on a scrim while open, closed by the
 * scrim and by `Escape`.
 */
import { fireEvent, screen } from "@testing-library/react";
import { afterEach, describe, expect, it, vi } from "vitest";
import { show } from "./SidebarTestHelpers";

afterEach(() => vi.unstubAllGlobals());

describe("the sidebar of a narrow window", () => {
  it("is not drawn while closed", () => {
    show({ narrow: true, open: false });
    expect(screen.queryByRole("navigation", { name: "Pages" })).toBeNull();
  });

  it("is drawn over the page while open, and closes on the scrim and on Escape", async () => {
    const onClose = vi.fn();
    show({ narrow: true, open: true, onClose });
    await screen.findByRole("link", { name: "notes" });
    expect(screen.getByRole("navigation", { name: "Pages" })).toBeTruthy();
    fireEvent.click(screen.getByRole("button", { name: "Close the sidebar" }));
    expect(onClose).toHaveBeenCalledTimes(1);
    fireEvent.keyDown(window, { key: "Escape" });
    expect(onClose).toHaveBeenCalledTimes(2);
  });

  it("has the standard's hairline at its edge against the dimmed page", async () => {
    show({ narrow: true, open: true });
    await screen.findByRole("link", { name: "notes" });
    const edge = screen.getByRole("navigation", { name: "Pages" }).className;
    expect(edge).toContain("border-r");
    expect(edge).toContain("border-hairline");
  });

  it("stays beside the page in a wide window", () => {
    show({ narrow: false, open: false });
    expect(screen.getByRole("navigation", { name: "Pages" })).toBeTruthy();
    expect(screen.queryByRole("button", { name: "Close the sidebar" })).toBeNull();
  });
});
