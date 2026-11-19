// @vitest-environment jsdom
/** A toned tag is a state: an icon with a name and a word, never a coloured pill. */
import { cleanup, render, screen } from "@testing-library/react";
import { afterEach, describe, expect, it } from "vitest";
import { Tag } from "./Tag";

afterEach(cleanup);

describe("a tag", () => {
  it("says a toned state in the icon's name and in text, without a pill", () => {
    const names = {
      progress: "In progress",
      attention: "Not started",
      complete: "Done",
      error: "Blocked",
    } as const;
    for (const [tone, name] of Object.entries(names)) {
      const { container } = render(<Tag tone={tone as keyof typeof names}>word</Tag>);
      expect(screen.getByRole("img", { name })).toBeTruthy();
      expect(screen.getByText("word")).toBeTruthy();
      expect(container.querySelector(".rounded-full")).toBeNull();
      cleanup();
    }
  });

  it("keeps a plain tag as a label without an icon", () => {
    render(<Tag>word</Tag>);
    expect(screen.queryByRole("img")).toBeNull();
  });
});
