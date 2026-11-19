/** The address as a page's filters: set, changed and removed in place, with the other parameters left alone. */
import { act, renderHook } from "@testing-library/react";
import type { ReactNode } from "react";
import { MemoryRouter, useLocation } from "react-router";
import { describe, expect, it } from "vitest";
import { useQueryParams } from "./useQueryParams";

function setup(initial: string) {
  const wrapper = ({ children }: { children: ReactNode }) => (
    <MemoryRouter initialEntries={[initial]}>{children}</MemoryRouter>
  );
  return renderHook(() => ({ query: useQueryParams(), location: useLocation() }), { wrapper });
}

describe("useQueryParams", () => {
  it("sets a parameter, changes it and removes it with the empty string", () => {
    const view = setup("/page?tab=a");
    act(() => view.result.current.query[1]({ agent: "Coder" }));
    expect(view.result.current.location.search).toBe("?tab=a&agent=Coder");
    act(() => view.result.current.query[1]({ agent: "Reviewer", tab: "" }));
    expect(view.result.current.location.search).toBe("?agent=Reviewer");
    expect(view.result.current.query[0].get("agent")).toBe("Reviewer");
  });
});
