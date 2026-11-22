// @vitest-environment jsdom
/** A sheet at the right edge starts below the page header, where the desktop window's buttons are, and never covers them. */
import { cleanup, render, screen } from "@testing-library/react";
import { afterEach, describe, expect, it } from "vitest";
import { SheetFrame } from "./Sheet";

afterEach(cleanup);

describe("the sheet", () => {
  it("starts 48px down, below the page header, and not at the top of the page", () => {
    render(
      <SheetFrame label="Sheet" title="Packages" closeLabel="Close" onClose={() => {}}>
        body
      </SheetFrame>,
    );
    const classes = screen.getByRole("dialog", { name: "Sheet" }).className.split(" ");
    expect(classes).toContain("top-12");
    expect(classes).toContain("bottom-0");
    expect(classes).not.toContain("inset-y-0");
    expect(classes).not.toContain("top-0");
  });
});
