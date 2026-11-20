/**
 * The strip for the screens outside the shell: it moves the window and closes it, and is not there in a browser.
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

  it("is a drag region with a close button", () => {
    const bridge = { ...fakeBridge(), close: vi.fn() };
    render(<WindowBar bridge={bridge} />);
    expect(screen.getByTestId("window-bar").hasAttribute("data-tauri-drag-region")).toBe(true);
    expect(screen.queryByRole("button", { name: "Minimize" })).toBeNull();
    fireEvent.click(screen.getByRole("button", { name: "Close" }));
    expect(bridge.close).toHaveBeenCalledOnce();
  });
});
