/**
 * The strip for the screens outside the shell: it moves the window, minimizes, maximizes and closes it, and is not there in a browser.
 */
import { cleanup, fireEvent, render, screen } from "@testing-library/react";
import { afterEach, describe, expect, it, vi } from "vitest";
import { fakeBridge } from "./fakeBridge";
import { WindowBar } from "./WindowBar";

afterEach(cleanup);

describe("WindowBar", () => {
  it("draws nothing in a browser", () => {
    render(<WindowBar bridge={undefined} />);
    expect(screen.queryByTestId("window-bar")).toBeNull();
  });

  it("is a drag region with the three controls of the shell", () => {
    const bridge = { ...fakeBridge(), minimize: vi.fn(), toggleMaximize: vi.fn(), close: vi.fn() };
    render(<WindowBar bridge={bridge} />);
    expect(screen.getByTestId("window-bar").hasAttribute("data-tauri-drag-region")).toBe(true);
    expect(screen.getAllByRole("button").map((b) => b.getAttribute("aria-label"))).toEqual([
      "Minimize",
      "Maximize",
      "Close",
    ]);
    fireEvent.click(screen.getByRole("button", { name: "Minimize" }));
    fireEvent.click(screen.getByRole("button", { name: "Maximize" }));
    fireEvent.click(screen.getByRole("button", { name: "Close" }));
    expect(bridge.minimize).toHaveBeenCalledOnce();
    expect(bridge.toggleMaximize).toHaveBeenCalledOnce();
    expect(bridge.close).toHaveBeenCalledOnce();
  });
});
