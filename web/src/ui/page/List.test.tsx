// @vitest-environment jsdom
/**
 * A list beside a detail is two columns while there is something to choose from. When the list is empty there is
 * nothing to choose, so the column goes and the empty state — which holds the button that makes the first row — takes
 * the whole width rather than sitting in a narrow column beside a blank one.
 */
import { cleanup, render, screen } from "@testing-library/react";
import { afterEach, describe, expect, it } from "vitest";
import { Empty } from "./Empty";
import { ListCard, ListDetail } from "./List";

afterEach(cleanup);

describe("a list beside a detail", () => {
  it("keeps the column while there are rows to choose from", () => {
    render(
      <ListDetail
        list={[<ListCard key="a" title="docs" line="stdio" selected onClick={() => {}} />]}
        detail={<p>How it runs</p>}
      />,
    );
    expect(screen.getByRole("list")).toBeTruthy();
    expect(screen.getByRole("listitem")).toBeTruthy();
  });

  it("gives the whole width to the empty state when there is nothing to list", () => {
    const { container } = render(
      <ListDetail
        list={[]}
        detail={<Empty title="No servers yet" body="Add one to name its command." />}
      />,
    );
    // Nothing is listed, and nothing holds a column open beside the empty state.
    expect(screen.queryByRole("list")).toBeNull();
    expect(screen.getByText("No servers yet")).toBeTruthy();
    const panes = Array.from(container.querySelectorAll(":scope > div > div"));
    expect(panes).toHaveLength(1);
    expect(panes[0]?.className).toContain("flex-1");
  });

  it("keeps the column while the list is still being read", () => {
    render(<ListDetail list={<p>Reading the servers</p>} detail={<p>Nothing chosen</p>} />);
    expect(screen.getByText("Reading the servers")).toBeTruthy();
    expect(screen.getByText("Nothing chosen")).toBeTruthy();
  });
});
