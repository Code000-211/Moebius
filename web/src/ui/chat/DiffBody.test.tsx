// @vitest-environment jsdom
/**
 * The body of a changed file sits on the dark colour of code, so its lines, its Copy button and its note of left-out
 * lines are drawn with the colours of code rather than the page's text colours.
 */
import { cleanup, render, screen } from "@testing-library/react";
import { afterEach, describe, expect, it } from "vitest";
import { DiffBody } from "./DiffBody";

afterEach(cleanup);

describe("DiffBody", () => {
  it("draws its lines, button and note in the colours of code", () => {
    const many = Array.from({ length: 210 }, (_, i) => `line ${i}`).join("\n");
    const { container } = render(<DiffBody changes={[{ oldText: "old", newText: many }]} />);
    expect(container.firstElementChild?.className).toContain("bg-code-bg");
    // Positioned, so the steps' line of a turn runs behind the box, not through it.
    expect(container.firstElementChild?.className).toMatch(/(^| )relative( |$)/);
    expect(container.querySelector("pre")?.className).toContain("text-code-text");
    expect(screen.getByRole("button", { name: "Copy diff" }).className).toContain("text-code-head");
    expect(screen.getByText(/left out/).className).toContain("text-code-head");
    expect(container.innerHTML).not.toMatch(/text-text|text-secondary|bg-code(?!-)/);
  });
});
