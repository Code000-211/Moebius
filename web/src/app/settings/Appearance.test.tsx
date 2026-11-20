// @vitest-environment jsdom
import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { cleanup, fireEvent, render as mount, screen, waitFor } from "@testing-library/react";
import type { ReactElement } from "react";
import { afterEach, describe, expect, it, vi } from "vitest";
import { start } from "../../ui/lib/effects";
import { Appearance } from "./Appearance";

/** Renders in a query cache, as the page has. */
function render(element: ReactElement) {
  const client = new QueryClient({ defaultOptions: { queries: { retry: false } } });
  return mount(<QueryClientProvider client={client}>{element}</QueryClientProvider>);
}

afterEach(() => {
  cleanup();
  localStorage.clear();
});

describe("Appearance", () => {
  it("lays Theme, Reduce motion and Blur on menus out as rows with switches", () => {
    render(<Appearance />);
    expect(screen.getByRole("radiogroup", { name: "Theme" })).toBeTruthy();
    const motion = screen.getByRole("switch", { name: "Reduce motion" });
    expect(motion.getAttribute("aria-checked")).toBe("false");
    fireEvent.click(motion);
    expect(motion.getAttribute("aria-checked")).toBe("true");
    expect(document.documentElement.dataset.reduceMotion).toBe("true");
    fireEvent.click(motion);
  });

  it("says blur is off automatically without a GPU and leaves the switch off", () => {
    start(); // jsdom has no WebGL: no GPU
    render(<Appearance />);
    const blur = screen.getByRole("switch", { name: "Blur on menus" }) as HTMLButtonElement;
    expect(blur.disabled).toBe(true);
    expect(blur.getAttribute("aria-checked")).toBe("false");
    expect(screen.getByText("Off automatically without GPU")).toBeTruthy();
  });

  it("saves Show thinking expanded to the server when the switch is moved", async () => {
    const sent: unknown[] = [];
    vi.stubGlobal("fetch", async (_url: string, init?: RequestInit) => {
      if (init?.method === "PUT") sent.push(JSON.parse(String(init.body)));
      return new Response(JSON.stringify({ show_thinking_expanded: sent.length > 0 }));
    });
    render(<Appearance />);
    const row = screen.getByRole("switch", { name: "Show thinking expanded" });
    expect(row.getAttribute("aria-checked")).toBe("false");
    fireEvent.click(row);
    await waitFor(() => expect(sent).toEqual([{ show_thinking_expanded: true }]));
    expect(row.getAttribute("aria-checked")).toBe("true");
    vi.unstubAllGlobals();
  });
});
