/**
 * The desktop app's window has no system frame, so the web app draws its controls; in a browser it draws none.
 */
import { cleanup, fireEvent, render, screen } from "@testing-library/react";
import { afterEach, describe, expect, it, vi } from "vitest";
import { WindowControls } from "./WindowControls";

afterEach(cleanup);

describe("WindowControls", () => {
  it("draws nothing in a browser", () => {
    render(<WindowControls bridge={undefined} />);
    expect(screen.queryByTestId("window-bar")).toBeNull();
  });

  it("moves the window with the strip and calls the app for each control", () => {
    const bridge = { minimize: vi.fn(), toggleMaximize: vi.fn(), close: vi.fn() };
    render(<WindowControls bridge={bridge} />);
    expect(screen.getByTestId("window-bar").hasAttribute("data-tauri-drag-region")).toBe(true);
    fireEvent.click(screen.getByRole("button", { name: "Minimize" }));
    fireEvent.click(screen.getByRole("button", { name: "Maximize" }));
    fireEvent.click(screen.getByRole("button", { name: "Close" }));
    expect(bridge.minimize).toHaveBeenCalledOnce();
    expect(bridge.toggleMaximize).toHaveBeenCalledOnce();
    expect(bridge.close).toHaveBeenCalledOnce();
  });
});
