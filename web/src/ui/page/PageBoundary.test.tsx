// @vitest-environment jsdom
/**
 * A page is fetched when its route opens, so the fetch can fail while the app around it is fine. The frame that
 * catches that says so and keeps the shell: the router's own screen would take the sidebar and the banner of the
 * connection with it, leaving a stack trace where a person expected a page.
 */
import { cleanup, render, screen } from "@testing-library/react";
import { afterEach, describe, expect, it, vi } from "vitest";
import { PageBoundary } from "./PageBoundary";

afterEach(cleanup);

/** A page whose file never arrived. */
function Missing(): never {
  throw new Error("Failed to fetch dynamically imported module: /assets/FleetPage-abc.js");
}

describe("the frame around a page", () => {
  it("says the page could not be loaded and keeps what stands beside it", () => {
    // React and the frame both report the failure; the test is about what the screen shows.
    const reported = vi.spyOn(console, "error").mockImplementation(() => {});
    render(
      <div>
        <p>Pages</p>
        <PageBoundary>
          <Missing />
        </PageBoundary>
      </div>,
    );
    const line = screen.getByRole("alert");
    expect(line.textContent).toContain("The page could not be loaded");
    expect(screen.getByRole("button", { name: "Reload" })).toBeTruthy();
    expect(screen.getByText("Pages")).toBeTruthy();
    expect(reported).toHaveBeenCalled();
    reported.mockRestore();
  });

  it("leaves the page alone when nothing failed", () => {
    render(
      <PageBoundary>
        <p>Fleet</p>
      </PageBoundary>,
    );
    expect(screen.getByText("Fleet")).toBeTruthy();
    expect(screen.queryByRole("alert")).toBeNull();
  });
});
