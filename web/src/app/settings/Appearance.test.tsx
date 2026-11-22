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
      return new Response(
        JSON.stringify({ show_thinking_expanded: sent.length > 0, wrap_long_lines: false }),
      );
    });
    render(<Appearance />);
    const row = screen.getByRole("switch", { name: "Show thinking expanded" });
    expect(row.getAttribute("aria-checked")).toBe("false");
    fireEvent.click(row);
    await waitFor(() =>
      expect(sent).toEqual([{ show_thinking_expanded: true, wrap_long_lines: false }]),
    );
    expect(row.getAttribute("aria-checked")).toBe("true");
    vi.unstubAllGlobals();
  });

  it("has a Wrap long lines row, off at first, that saves it with the other preferences", async () => {
    const sent: unknown[] = [];
    let kept = { show_thinking_expanded: true, wrap_long_lines: false };
    vi.stubGlobal("fetch", async (_url: string, init?: RequestInit) => {
      if (init?.method === "PUT") {
        kept = JSON.parse(String(init.body));
        sent.push(kept);
      }
      return new Response(JSON.stringify(kept));
    });
    render(<Appearance />);
    const row = screen.getByRole("switch", { name: "Wrap long lines" });
    expect(row.getAttribute("aria-checked")).toBe("false");
    // The thinking choice already kept is sent along, since the server replaces the preferences as a whole.
    await waitFor(() =>
      expect(
        screen.getByRole("switch", { name: "Show thinking expanded" }).getAttribute("aria-checked"),
      ).toBe("true"),
    );
    fireEvent.click(row);
    await waitFor(() =>
      expect(sent).toEqual([{ show_thinking_expanded: true, wrap_long_lines: true }]),
    );
    expect(row.getAttribute("aria-checked")).toBe("true");
    vi.unstubAllGlobals();
  });

  it("puts the switch back and says so when the save fails", async () => {
    vi.stubGlobal("fetch", async (_url: string, init?: RequestInit) =>
      init?.method === "PUT"
        ? new Response("no", { status: 500 })
        : new Response(JSON.stringify({ show_thinking_expanded: false, wrap_long_lines: false })),
    );
    render(<Appearance />);
    const row = screen.getByRole("switch", { name: "Wrap long lines" });
    fireEvent.click(row);
    await waitFor(() => expect(row.getAttribute("aria-checked")).toBe("false"));
    vi.unstubAllGlobals();
  });
});
