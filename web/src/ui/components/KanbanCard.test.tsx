// @vitest-environment jsdom
/**
 * A board card as a keyboard and a screen reader meet it. The card is a group and not a button: naming a button makes
 * everything inside it presentational, and a card holds the link to its issue and the badge of its pull request, which
 * a person has to be able to reach. Its title is the control that opens it, and the card itself stays the handle the
 * drag library picks up — which is why the group is focusable too.
 */
import { cleanup, fireEvent, render, screen, within } from "@testing-library/react";
import { afterEach, describe, expect, it, vi } from "vitest";
import { KanbanCard, KanbanCards, KanbanProvider } from "./KanbanBoard";
import { KanbanCardContent } from "./KanbanCardContent";

afterEach(cleanup);

function Card({ onOpen, preview }: { onOpen: () => void; preview?: string | null }) {
  return (
    <KanbanProvider onDragEnd={() => {}} renderOverlay={() => null}>
      <KanbanCards>
        <KanbanCard
          id="12"
          column="Not started"
          name="#12 Fix the parser"
          testId="card-12"
          onOpen={onOpen}
        >
          <KanbanCardContent
            displayId="#12"
            issueUrl="https://github.com/acme/demo/issues/12"
            title="Fix the parser"
            preview={preview}
            detail="Coder · round 1 of 5"
            badges={[{ name: "Your turn", tone: "attention" }]}
            onOpen={onOpen}
          />
        </KanbanCard>
      </KanbanCards>
    </KanbanProvider>
  );
}

describe("a board card", () => {
  it("is a group, so the links it holds stay in the page", () => {
    render(<Card onOpen={() => {}} />);
    const group = screen.getByRole("group", { name: "#12 Fix the parser" });
    expect(group.getAttribute("aria-roledescription")).toBe("card");
    expect(within(group).getByRole("link", { name: "Issue #12 on its forge" })).toBeTruthy();
    expect(within(group).getByText("Your turn")).toBeTruthy();
  });

  it("opens from its title, once", () => {
    const onOpen = vi.fn();
    render(<Card onOpen={onOpen} />);
    fireEvent.click(screen.getByRole("button", { name: "Fix the parser" }));
    expect(onOpen).toHaveBeenCalledTimes(1);
  });

  it("shows what the issue asks for, below the title", () => {
    render(<Card onOpen={() => {}} preview="The board is cramped on a phone" />);
    const preview = screen.getByText("The board is cramped on a phone");
    expect(preview.className).toContain("clamp-3");
  });

  it("holds nothing where a blank issue would have left a gap", () => {
    const { container } = render(<Card onOpen={() => {}} preview="   " />);
    expect(container.querySelector(".clamp-3")).toBeNull();
  });

  it("opens with Enter on the card itself, where the drag handle takes focus", () => {
    const onOpen = vi.fn();
    render(<Card onOpen={onOpen} />);
    const group = screen.getByRole("group", { name: "#12 Fix the parser" });
    expect(group.getAttribute("tabindex")).toBe("0");
    fireEvent.keyDown(group, { key: "Enter" });
    expect(onOpen).toHaveBeenCalledTimes(1);
  });
});
