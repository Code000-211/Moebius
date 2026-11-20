// @vitest-environment jsdom
/** The icon module draws every icon as one regular-weight glyph at the size it is given. */
import { render } from "@testing-library/react";
import { describe, expect, it } from "vitest";
import * as icons from "./index";

describe("the icon module", () => {
  it("exports icons by meaning, each a component that draws one svg", () => {
    const names = Object.keys(icons);
    expect(names).toContain("AddIcon");
    expect(names).toContain("SearchIcon");
    for (const name of names) {
      const Icon = (icons as Record<string, React.ElementType>)[name] as React.ElementType;
      const { container, unmount } = render(<Icon size={15} />);
      expect(container.querySelectorAll("svg"), name).toHaveLength(1);
      unmount();
    }
  });
});
