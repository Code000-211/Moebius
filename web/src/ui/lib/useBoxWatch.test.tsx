// @vitest-environment jsdom
/**
 * The watching of a box and its children. A child that left the box must not stay observed (and so referenced) for as
 * long as the page lasts: the board's lanes come and go with every filter.
 */
import { act, render } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { useBoxWatch } from "./useBoxWatch";

const observed = new Set<Element>();
class FakeObserver {
  observe(el: Element) {
    observed.add(el);
  }
  unobserve(el: Element) {
    observed.delete(el);
  }
  disconnect() {
    observed.clear();
  }
}

beforeEach(() => {
  observed.clear();
  vi.stubGlobal("ResizeObserver", FakeObserver);
});
afterEach(() => vi.unstubAllGlobals());

function Box({ lanes }: { lanes: string[] }) {
  const { box } = useBoxWatch<HTMLDivElement>(() => {}, { children: true });
  return (
    <div ref={box} data-testid="box">
      {lanes.map((lane) => (
        <p key={lane} data-lane={lane}>
          {lane}
        </p>
      ))}
    </div>
  );
}

describe("watching a box and its children", () => {
  it("lets go of a child that left the box", () => {
    const { container, rerender } = render(<Box lanes={["a", "b"]} />);
    const lane = (name: string) => container.querySelector(`[data-lane="${name}"]`);
    const b = lane("b");
    expect(b && observed.has(b)).toBe(true);
    // 1 box + 2 lanes.
    expect(observed.size).toBe(3);
    act(() => rerender(<Box lanes={["a", "c"]} />));
    expect(observed.size).toBe(3);
    expect(b && observed.has(b)).toBe(false);
    const c = lane("c");
    expect(c && observed.has(c)).toBe(true);
  });
});
