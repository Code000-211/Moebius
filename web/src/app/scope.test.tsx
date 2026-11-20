// @vitest-environment jsdom
import { act, cleanup, renderHook } from "@testing-library/react";
import type { ReactNode } from "react";
import { MemoryRouter, useLocation } from "react-router";
import { afterEach, describe, expect, it } from "vitest";
import { useDays } from "./scope";

afterEach(cleanup);

function at(path: string) {
  const wrapper = ({ children }: { children: ReactNode }) => (
    <MemoryRouter initialEntries={[path]}>{children}</MemoryRouter>
  );
  return renderHook(
    () => {
      const days = useDays([1, 7, 30], 1);
      return { days, search: useLocation().search };
    },
    { wrapper },
  );
}

describe("the window of a page", () => {
  it("is kept in the address, so a reload and a link keep it", () => {
    const view = at("/usage");
    expect(view.result.current.days[0]).toBe(1);
    act(() => view.result.current.days[1](7));
    expect(view.result.current.search).toBe("?days=7");
    expect(view.result.current.days[0]).toBe(7);
    // The same address read afresh, as after a reload.
    expect(at("/usage?days=7").result.current.days[0]).toBe(7);
  });

  it("leaves the default out of the address and falls back from a value with no chip", () => {
    const view = at("/usage?days=7");
    act(() => view.result.current.days[1](1));
    expect(view.result.current.search).toBe("");
    expect(at("/usage?days=5").result.current.days[0]).toBe(1);
  });
});
