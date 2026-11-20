// @vitest-environment jsdom
/** A state is an icon with a name and a word, never a coloured pill. */
import { cleanup, render, screen } from "@testing-library/react";
import { afterEach, describe, expect, it } from "vitest";
import { StateLabel } from "./StateLabel";

afterEach(cleanup);

describe("a state label", () => {
  it("says the state in the icon's name and in text, without a pill", () => {
    const { container } = render(<StateLabel tone="complete">A login is kept.</StateLabel>);
    expect(screen.getByRole("img", { name: "Done" })).toBeTruthy();
    expect(screen.getByText("A login is kept.")).toBeTruthy();
    expect(container.querySelector(".rounded-full")).toBeNull();
  });

  it("gives each tone an icon and a neutral fact none", () => {
    const names = { progress: "Running", attention: "Needs you", error: "Needs you" } as const;
    for (const [tone, name] of Object.entries(names)) {
      render(<StateLabel tone={tone as keyof typeof names}>word</StateLabel>);
      expect(screen.getByRole("img", { name })).toBeTruthy();
      cleanup();
    }
    render(<StateLabel>Read only</StateLabel>);
    expect(screen.queryByRole("img")).toBeNull();
  });
});
