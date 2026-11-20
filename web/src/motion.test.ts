// @vitest-environment jsdom
/** The reduce-motion switch: the attribute on the root follows it, it is remembered, and blocked storage does not stop it. */
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

beforeEach(() => {
  vi.resetModules();
  delete document.documentElement.dataset.reduceMotion;
});
afterEach(() => vi.unstubAllGlobals());

describe("reduce motion", () => {
  it("starts from what was saved and writes the attribute", async () => {
    vi.stubGlobal("localStorage", { getItem: () => "on", setItem: () => {} });
    const { start } = await import("./motion");
    expect(start()).toBe(true);
    expect(document.documentElement.dataset.reduceMotion).toBe("true");
  });

  it("is off when nothing was saved", async () => {
    vi.stubGlobal("localStorage", { getItem: () => null, setItem: () => {} });
    const { start } = await import("./motion");
    expect(start()).toBe(false);
    expect(document.documentElement.dataset.reduceMotion).toBeUndefined();
  });

  it("applies and remembers a switch, also with blocked storage", async () => {
    const saved: string[] = [];
    vi.stubGlobal("localStorage", {
      getItem: () => null,
      setItem: (_: string, v: string) => saved.push(v),
    });
    const { renderHook, act } = await import("@testing-library/react");
    const { useReduceMotion } = await import("./motion");
    const { result } = renderHook(() => useReduceMotion());
    act(() => result.current.setOn(true));
    expect(result.current.on).toBe(true);
    expect(document.documentElement.dataset.reduceMotion).toBe("true");
    expect(saved).toEqual(["on"]);
    const blocked = () => {
      throw new DOMException("blocked", "SecurityError");
    };
    vi.stubGlobal("localStorage", { getItem: blocked, setItem: blocked });
    act(() => result.current.setOn(false));
    expect(result.current.on).toBe(false);
    expect(document.documentElement.dataset.reduceMotion).toBeUndefined();
  });
});
