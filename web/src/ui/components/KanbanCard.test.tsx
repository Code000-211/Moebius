// @vitest-environment jsdom
/**
 * A board card: flat and quiet, and as a keyboard and a screen reader meet it. The card is a group and not a button:
 * its title is the control that opens it, and the card itself stays the handle the drag library picks up — which is why
 * the group is focusable too.
 */
import { cleanup, fireEvent, render, screen } from "@testing-library/react";
import { afterEach, describe, expect, it, vi } from "vitest";
import { KanbanCard, KanbanCards, KanbanProvider } from "./KanbanBoard";
import { KanbanCardContent } from "./KanbanCardContent";

afterEach(cleanup);

function Card({
  onOpen,
  activity,
  assignees,
}: {
  onOpen: () => void;
  activity?: { text: string; tone: "quiet" | "needs-you" } | null;
  assignees?: string[];
}) {
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
            title="Fix the parser"
            status="notStarted"
            activity={activity}
            assignees={assignees}
            onOpen={onOpen}
          />
        </KanbanCard>
      </KanbanCards>
    </KanbanProvider>
  );
}

describe("a board card", () => {
  it("is a group a keyboard and a screen reader can find", () => {
    render(<Card onOpen={() => {}} />);
    const group = screen.getByRole("group", { name: "#12 Fix the parser" });
    expect(group.getAttribute("aria-roledescription")).toBe("card");
  });

  it("is flat when idle: its id and title only, no activity line and no shadow", () => {
    render(<Card onOpen={() => {}} />);
    const group = screen.getByTestId("card-12");
    expect(screen.queryByTestId("card-activity")).toBeNull();
    expect(group.className).not.toMatch(/shadow/);
    // The first child holds the icon and the id, the second is the title.
    const body = group.firstElementChild as HTMLElement;
    expect(body.children).toHaveLength(2);
    expect(body.children[0]?.textContent).toContain("#12");
    expect(body.children[1]?.textContent).toBe("Fix the parser");
  });

  it("shows a grey line while an agent works", () => {
    render(<Card onOpen={() => {}} activity={{ text: "Coder · code", tone: "quiet" }} />);
    const line = screen.getByTestId("card-activity");
    expect(line.textContent).toBe("Coder · code");
    expect(line.className).toContain("text-secondary");
  });

  it("shows an orange line when it needs a person", () => {
    render(<Card onOpen={() => {}} activity={{ text: "Plan ready", tone: "needs-you" }} />);
    expect(screen.getByTestId("card-activity").className).toContain("text-needs-you-text");
  });

  it("shows an avatar only when a person owns it", () => {
    const { rerender } = render(<Card onOpen={() => {}} />);
    expect(screen.queryByRole("img", { name: /Given to/ })).toBeNull();
    rerender(<Card onOpen={() => {}} assignees={["ada"]} />);
    expect(screen.getByRole("img", { name: "Given to @ada" })).toBeTruthy();
  });

  it("opens from its title, once", () => {
    const onOpen = vi.fn();
    render(<Card onOpen={onOpen} />);
    fireEvent.click(screen.getByRole("button", { name: "Fix the parser" }));
    expect(onOpen).toHaveBeenCalledTimes(1);
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
