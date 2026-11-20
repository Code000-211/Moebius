// @vitest-environment jsdom
/** The marks name their state to a screen reader and by shape, so no state rests on colour alone. */
import { cleanup, render, screen } from "@testing-library/react";
import { afterEach, describe, expect, it } from "vitest";
import {
  LabelDot,
  PRIORITIES,
  PriorityBars,
  STATUS_KIND_NAMES,
  STATUS_KINDS,
  STATUS_STATES,
  StatusIcon,
  type StatusKind,
  type StatusState,
  statusKind,
} from "./StatusIcon";

afterEach(cleanup);

const HUE: Record<StatusKind, string> = {
  needsYou: "--needs-you",
  running: "--running",
  inReview: "--status-review",
  done: "--status-done",
  notStarted: "--status-todo",
};

describe("a status icon", () => {
  it("is one of five, each named for its state", () => {
    expect(STATUS_KINDS).toHaveLength(5);
    for (const kind of STATUS_KINDS) {
      render(<StatusIcon state={kind} />);
      expect(screen.getByRole("img", { name: STATUS_KIND_NAMES[kind] })).toBeTruthy();
      cleanup();
    }
  });

  it("differs in shape for every icon, not only in colour", () => {
    const shapes = STATUS_KINDS.map((kind) => {
      const { container } = render(<StatusIcon state={kind} />);
      const shape = container.querySelector("svg")?.innerHTML.replace(/var\(--[a-z-]+\)/g, "");
      cleanup();
      return shape;
    });
    expect(new Set(shapes).size).toBe(5);
  });

  it("draws each icon in its own token, never a literal colour, so both themes work", () => {
    for (const kind of STATUS_KINDS) {
      const { container } = render(<StatusIcon state={kind} />);
      expect(container.innerHTML).not.toMatch(/#[0-9a-f]{3,6}/i);
      expect(container.innerHTML).toContain(`var(${HUE[kind]})`);
      cleanup();
    }
  });

  it("maps every state a caller names to one of the five", () => {
    const expected: Record<StatusState, StatusKind> = {
      backlog: "notStarted",
      notStarted: "notStarted",
      inProgress: "running",
      running: "running",
      inReview: "inReview",
      readyToMerge: "inReview",
      done: "done",
      blocked: "needsYou",
      needsYou: "needsYou",
    };
    for (const state of STATUS_STATES) {
      const { container } = render(<StatusIcon state={state} />);
      expect(
        container.querySelector(`[data-testid="status-${expected[state]}"]`),
        state,
      ).toBeTruthy();
      cleanup();
    }
  });

  it("takes a name that is no state for not started", () => {
    expect(statusKind("Archived")).toBe("notStarted");
    const { container } = render(<StatusIcon state={"Archived" as StatusState} />);
    expect(container.querySelector('[data-testid="status-notStarted"]')).toBeTruthy();
  });

  it("has no animation or spin class on any element", () => {
    for (const state of STATUS_STATES) {
      const { container } = render(<StatusIcon state={state} />);
      for (const el of container.querySelectorAll("*")) {
        expect(el.getAttribute("class") ?? "", state).not.toMatch(/animate|spin|transition|pulse/);
      }
      expect(container.innerHTML).not.toMatch(/<animate|animateTransform/);
      cleanup();
    }
  });

  it("hides itself when a word stands beside it", () => {
    const { container } = render(<StatusIcon state="blocked" decorative />);
    expect(screen.queryByRole("img")).toBeNull();
    expect(container.querySelector("svg")?.getAttribute("aria-hidden")).toBe("true");
  });
});

describe("priority bars", () => {
  it("name every level and differ in shape", () => {
    const shapes = PRIORITIES.map((p) => {
      const { container } = render(<PriorityBars priority={p} />);
      expect(screen.getAllByRole("img")).toHaveLength(1);
      const shape = container.querySelector("svg")?.innerHTML;
      cleanup();
      return shape;
    });
    expect(new Set(shapes).size).toBe(PRIORITIES.length);
  });

  it("fill as many bars as the level", () => {
    const { container } = render(<PriorityBars priority="medium" />);
    expect(container.querySelectorAll('[data-bar="filled"]')).toHaveLength(2);
    expect(container.querySelectorAll('[data-bar="empty"]')).toHaveLength(1);
  });

  it("colour only urgent", () => {
    for (const p of PRIORITIES) {
      const { container } = render(<PriorityBars priority={p} />);
      expect(container.innerHTML.includes("--status-error")).toBe(p === "urgent");
      cleanup();
    }
    render(<PriorityBars priority="urgent" />);
    expect(screen.getByRole("img", { name: "Urgent" })).toBeTruthy();
  });
});

describe("a label dot", () => {
  it("is a dot and a word, the word being the name", () => {
    render(<LabelDot name="Design" color="var(--accent)" />);
    expect(screen.getByText("Design")).toBeTruthy();
    const dot = screen.getByTestId("label-dot");
    expect(dot.getAttribute("aria-hidden")).toBe("true");
    expect(dot.getAttribute("style")).toContain("var(--accent)");
  });
});
