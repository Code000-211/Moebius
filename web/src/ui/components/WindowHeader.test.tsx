// @vitest-environment jsdom
/** The top bar of a window: what leads, the title, the trailing controls, and the close icon always last. */
import { cleanup, fireEvent, render, screen } from "@testing-library/react";
import { afterEach, describe, expect, it, vi } from "vitest";
import { WindowHeader } from "./WindowHeader";

afterEach(cleanup);

describe("WindowHeader", () => {
  it("is 48px high with a hairline below and the close icon after the trailing controls", () => {
    const onClose = vi.fn();
    render(
      <WindowHeader
        leading="Retro proposal"
        title="Decide"
        trailing={<a href="/retro">Open as page</a>}
        onClose={onClose}
      />,
    );
    const bar = screen.getByTestId("window-header");
    expect(bar.className).toContain("h-12");
    expect(bar.className).toContain("border-b");
    const names = [...bar.querySelectorAll("a, button")].map(
      (each) => each.getAttribute("aria-label") ?? each.textContent,
    );
    expect(names).toEqual(["Open as page", "Close"]);
    fireEvent.click(screen.getByRole("button", { name: "Close" }));
    expect(onClose).toHaveBeenCalledOnce();
    expect(screen.getByText("Retro proposal")).toBeTruthy();
    expect(screen.getByText("Decide")).toBeTruthy();
  });

  it("has no close icon when there is no way out, and names it as asked", () => {
    const { rerender } = render(<WindowHeader leading="MOE-1" />);
    expect(screen.queryByRole("button")).toBeNull();
    rerender(<WindowHeader leading="MOE-1" onClose={() => {}} closeLabel="Close the inspector" />);
    expect(screen.getByRole("button", { name: "Close the inspector" })).toBeTruthy();
  });
});
