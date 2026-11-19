// @vitest-environment jsdom
/** The marks name their state to a screen reader and by shape, so no state rests on colour alone. */
import { cleanup, render, screen } from "@testing-library/react";
import { afterEach, describe, expect, it } from "vitest";
import {
  LabelDot,
  PRIORITIES,
  PriorityBars,
  RunRing,
  STATUS_NAMES,
  STATUS_STATES,
  StatusIcon,
} from "./StatusIcon";

afterEach(cleanup);

describe("a status icon", () => {
  it("is named for its state", () => {
    for (const state of STATUS_STATES) {
      render(<StatusIcon state={state} />);
      expect(screen.getByRole("img", { name: STATUS_NAMES[state] })).toBeTruthy();
      cleanup();
    }
  });

  it("differs in shape for every state, not only in colour", () => {
    const shapes = STATUS_STATES.map((state) => {
      const { container } = render(<StatusIcon state={state} />);
      const shape = container.querySelector("svg")?.innerHTML.replace(/var\(--[a-z-]+\)/g, "");
      cleanup();
      return shape;
    });
    expect(new Set(shapes).size).toBe(STATUS_STATES.length);
  });

  it("draws in theme tokens only, so both themes work", () => {
    const { container } = render(<StatusIcon state="done" />);
    expect(container.innerHTML).not.toMatch(/#[0-9a-f]{3,6}/i);
    expect(container.innerHTML).toContain("var(--status-done)");
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

describe("a run ring", () => {
  it("is named, turns, and stops turning for reduced motion", () => {
    render(<RunRing />);
    const ring = screen.getByRole("img", { name: "Running" });
    const cls = ring.getAttribute("class") ?? "";
    expect(cls).toContain("animate-spin");
    expect(cls).toContain("motion-reduce:animate-none");
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
