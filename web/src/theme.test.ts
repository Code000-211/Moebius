// @vitest-environment jsdom
/**
 * The theme with storage that refuses every access, as a browser with site data blocked does. The choice a person
 * makes has to stay the preference, and a change of the system must not take it away.
 */
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

/** A stand-in for `matchMedia` whose answer a test can flip and announce. */
function fakeMedia(light: boolean) {
  const listeners = new Set<() => void>();
  const query = {
    matches: light,
    addEventListener: (_: string, listener: () => void) => listeners.add(listener),
    removeEventListener: (_: string, listener: () => void) => listeners.delete(listener),
  };
  vi.stubGlobal("matchMedia", () => query);
  return {
    flip(to: boolean) {
      query.matches = to;
      for (const listener of listeners) listener();
    },
  };
}

beforeEach(() => {
  vi.resetModules();
  delete document.documentElement.dataset.theme;
});
afterEach(() => vi.unstubAllGlobals());

describe("the theme with blocked storage", () => {
  it("keeps a picked theme as the preference, and keeps it when the system changes", async () => {
    const media = fakeMedia(false);
    const blocked = () => {
      throw new DOMException("blocked", "SecurityError");
    };
    vi.stubGlobal("localStorage", { getItem: blocked, setItem: blocked });
    const { renderHook, act } = await import("@testing-library/react");
    const { start, useTheme } = await import("./theme");
    expect(start()).toBe("system");
    const { result } = renderHook(() => useTheme());
    expect(result.current).toMatchObject({ preference: "system", theme: "dark" });

    act(() => result.current.setPreference("light"));
    expect(result.current).toMatchObject({ preference: "light", theme: "light" });
    expect(document.documentElement.dataset.theme).toBe("light");

    // The system flips: the pick is not thrown away.
    act(() => media.flip(false));
    act(() => media.flip(true));
    expect(result.current).toMatchObject({ preference: "light", theme: "light" });
    act(() => media.flip(false));
    expect(document.documentElement.dataset.theme).toBe("light");
    expect(result.current.preference).toBe("light");
  });

  it("follows the system again when that is picked", async () => {
    const media = fakeMedia(false);
    const blocked = () => {
      throw new DOMException("blocked", "SecurityError");
    };
    vi.stubGlobal("localStorage", { getItem: blocked, setItem: blocked });
    const { renderHook, act } = await import("@testing-library/react");
    const { useTheme } = await import("./theme");
    const { result } = renderHook(() => useTheme());
    act(() => result.current.setPreference("light"));
    act(() => result.current.setPreference("system"));
    act(() => media.flip(true));
    expect(result.current).toMatchObject({ preference: "system", theme: "light" });
  });
});
