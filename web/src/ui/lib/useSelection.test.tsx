import { act, renderHook } from "@testing-library/react";
import { describe, expect, it } from "vitest";
import { useSelection } from "./useSelection";

describe("useSelection", () => {
  it("toggles one and chooses a range with shift, in page order", () => {
    const { result } = renderHook(() => useSelection([1, 2, 3, 4, 5]));
    act(() => result.current.toggle(2));
    act(() => result.current.toggle(4, true));
    expect(result.current.keys).toEqual([2, 3, 4]);
    act(() => result.current.toggle(3));
    expect(result.current.keys).toEqual([2, 4]);
  });

  it("forgets a key whose row left the page", () => {
    const { result, rerender } = renderHook(({ order }) => useSelection(order), {
      initialProps: { order: [1, 2, 3] },
    });
    act(() => result.current.toggle(1));
    act(() => result.current.toggle(3));
    rerender({ order: [1, 2] });
    expect(result.current.keys).toEqual([1]);
  });

  it("keeps the choice a right click lands in, and replaces it otherwise", () => {
    const { result } = renderHook(() => useSelection([1, 2, 3]));
    act(() => result.current.toggle(1));
    act(() => result.current.toggle(2));
    act(() => result.current.ensure(2));
    expect(result.current.keys).toEqual([1, 2]);
    act(() => result.current.ensure(3));
    expect(result.current.keys).toEqual([3]);
  });

  it("clears on Escape, but not while a field has the keys", () => {
    const { result } = renderHook(() => useSelection([1, 2]));
    act(() => result.current.toggle(1));
    const input = document.createElement("input");
    document.body.append(input);
    act(() => {
      input.dispatchEvent(new KeyboardEvent("keydown", { key: "Escape", bubbles: true }));
    });
    expect(result.current.keys).toEqual([1]);
    act(() => {
      document.body.dispatchEvent(new KeyboardEvent("keydown", { key: "Escape", bubbles: true }));
    });
    expect(result.current.keys).toEqual([]);
    input.remove();
  });
});
