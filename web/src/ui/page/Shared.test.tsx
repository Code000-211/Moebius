// @vitest-environment jsdom
/** The shared parts of a page carry no shadow, no card and no animation, and the header is one sticky line. */
import { cleanup, render } from "@testing-library/react";
import { afterEach, describe, expect, it } from "vitest";
import { Box } from "./Box";
import { Empty } from "./Empty";
import { PageHeader } from "./PageHeader";
import { Skeleton, SkeletonList } from "./Skeleton";

afterEach(cleanup);

describe("shared page parts", () => {
  it("draws a box as a hairline-separated section without shadow or outline", () => {
    const { container } = render(<Box title="Repositories">x</Box>);
    const cls = (container.firstElementChild as HTMLElement).className;
    expect(cls).toContain("border-t");
    expect(cls).not.toMatch(/shadow|outline|bg-surface|rounded/);
  });

  it("draws an empty state without a card or shadow", () => {
    const { container } = render(
      <Empty icon={<i />} title="Nothing needs you" action={<b>Go</b>} />,
    );
    const cls = (container.firstElementChild as HTMLElement).className;
    expect(cls).not.toMatch(/shadow|outline|bg-surface|rounded/);
  });

  it("draws a skeleton without any animation, rows of 36px", () => {
    const { container } = render(
      <>
        <Skeleton />
        <SkeletonList rows={2} />
      </>,
    );
    expect(container.innerHTML).not.toContain("animate-");
    const rows = container.querySelectorAll("[data-skeleton-row]");
    expect(rows).toHaveLength(2);
    expect((rows[0] as HTMLElement).className).toContain("h-row-list");
  });

  it("draws the header as one sticky 48px line, tabs and actions in slots, and no subtitle", () => {
    const { container, queryByText, getByText } = render(
      <PageHeader
        title="Fleet"
        context="a sentence"
        tabs={<span>tabs</span>}
        actions={<span>acts</span>}
      />,
    );
    const header = container.querySelector("header") as HTMLElement;
    expect(header.className).toContain("sticky");
    expect(header.className).toContain("min-h-12");
    expect(header.className).toContain("--window-buttons");
    expect(header.hasAttribute("data-tauri-drag-region")).toBe(true);
    expect(getByText("tabs")).toBeTruthy();
    expect(getByText("acts")).toBeTruthy();
    expect(queryByText("a sentence")).toBeNull();
  });
});
