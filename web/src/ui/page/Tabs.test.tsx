// @vitest-environment jsdom
/** The chosen tab is told by a 2px indigo underline and indigo text, never by a fill; the others are muted. */
import { cleanup, render, screen } from "@testing-library/react";
import { afterEach, describe, expect, it } from "vitest";
import { Tabs } from "./Tabs";

afterEach(cleanup);

describe("tabs", () => {
  it("underlines the chosen tab in 2px of the accent and fills nothing", () => {
    render(
      <Tabs
        tabs={[
          { value: "a", label: "Active" },
          { value: "b", label: "Finished" },
        ]}
        value="a"
        onChange={() => {}}
        label="Views"
        idBase="t"
      />,
    );
    const chosen = screen.getByRole("tab", { name: "Active" }).className;
    expect(chosen).toContain("after:h-0.5");
    expect(chosen).toContain("after:bg-accent");
    expect(chosen).toContain("text-accent-text");
    const other = screen.getByRole("tab", { name: "Finished" }).className;
    expect(other).toContain("text-muted");
    expect(other).not.toContain("after:");
    for (const tab of [chosen, other]) {
      expect(tab).not.toMatch(/(^| )bg-(selected|accent-tint|hover-fill)( |$)/);
      expect(tab).toContain("bg-transparent");
    }
  });
});
