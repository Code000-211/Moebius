import { render, screen } from "@testing-library/react";
import { describe, expect, it } from "vitest";
import { MarkLabel } from "./MarkLabel";

describe("a state label", () => {
  it("says a live run as the ring, the word and the agent, with the ring hidden from assistive technology", () => {
    const { container } = render(<MarkLabel mark="live" word="Live" detail="coder" />);
    expect(screen.getByText("Live")).toBeTruthy();
    expect(screen.getByText("· coder")).toBeTruthy();
    expect(container.querySelector('[data-testid="run-ring"]')?.getAttribute("aria-hidden")).toBe(
      "true",
    );
    expect(screen.queryByRole("img")).toBeNull();
  });

  it("says blocked with the blocked icon and a waiting line as grey text alone", () => {
    const { container, rerender } = render(<MarkLabel mark="blocked" word="Blocked" />);
    expect(container.querySelector('[data-testid="status-blocked"]')).toBeTruthy();
    rerender(<MarkLabel word="Waiting on #24" />);
    expect(container.querySelector("svg")).toBeNull();
    expect(screen.getByText("Waiting on #24").parentElement?.className).toContain("text-secondary");
    expect(container.innerHTML).not.toContain("rounded-full");
  });
});
