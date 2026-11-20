/**
 * The desktop app's window has no system frame, so the web app draws its controls once in the shell's panel; in a
 * browser it draws none.
 */
import { cleanup, fireEvent, render, screen } from "@testing-library/react";
import { afterEach, describe, expect, it, vi } from "vitest";
import { fakeBridge } from "./fakeBridge";
import { WindowControls } from "./WindowControls";

afterEach(cleanup);

describe("WindowControls", () => {
  it("draws nothing in a browser", () => {
    render(<WindowControls bridge={undefined} />);
    expect(screen.queryByTestId("window-controls")).toBeNull();
    expect(screen.queryByRole("button")).toBeNull();
  });

  it("draws one set of three buttons, not a bar across the window, and calls the app for each", () => {
    const bridge = { ...fakeBridge(), minimize: vi.fn(), toggleMaximize: vi.fn(), close: vi.fn() };
    render(<WindowControls bridge={bridge} />);
    expect(screen.getAllByRole("button").map((b) => b.getAttribute("aria-label"))).toEqual([
      "Minimize",
      "Maximize",
      "Close",
    ]);
    expect(screen.getByTestId("window-controls").className).not.toContain("fixed");
    fireEvent.click(screen.getByRole("button", { name: "Minimize" }));
    fireEvent.click(screen.getByRole("button", { name: "Maximize" }));
    fireEvent.click(screen.getByRole("button", { name: "Close" }));
    expect(bridge.minimize).toHaveBeenCalledOnce();
    expect(bridge.toggleMaximize).toHaveBeenCalledOnce();
    expect(bridge.close).toHaveBeenCalledOnce();
  });
});
