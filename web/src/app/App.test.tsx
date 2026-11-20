// @vitest-environment jsdom
/**
 * The screens outside the shell in the desktop app: the sign-in screen, the one for a server that does not answer and
 * the loader each have the strip that moves and closes the window, and the shell does not (it has its controls in its
 * panel). In a browser none of them draws it.
 */
import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { cleanup, render, screen } from "@testing-library/react";
import { MemoryRouter } from "react-router";
import { afterEach, describe, expect, it, vi } from "vitest";
import { fakeBridge } from "../desktop/fakeBridge";
import { App } from "./App";

afterEach(() => {
  cleanup();
  vi.unstubAllGlobals();
  delete window.moebiusDesktop;
});

function mount() {
  const queryClient = new QueryClient({ defaultOptions: { queries: { retry: false } } });
  return render(
    <QueryClientProvider client={queryClient}>
      <MemoryRouter>
        <App />
      </MemoryRouter>
    </QueryClientProvider>,
  );
}

describe("the screens outside the shell", () => {
  it("offer move and close on the sign-in screen in the desktop app", async () => {
    window.moebiusDesktop = fakeBridge();
    vi.stubGlobal(
      "fetch",
      vi.fn(async () => new Response("a token is needed", { status: 401 })),
    );
    mount();
    await screen.findByRole("heading", { name: "Moebius" });
    expect(screen.getByTestId("window-bar").hasAttribute("data-tauri-drag-region")).toBe(true);
    expect(screen.getByRole("button", { name: "Close" })).toBeTruthy();
  });

  it("offer move and close on the screen for a server that does not answer", async () => {
    window.moebiusDesktop = fakeBridge();
    vi.stubGlobal(
      "fetch",
      vi.fn(async () => {
        throw new TypeError("Failed to fetch");
      }),
    );
    mount();
    await screen.findByRole("heading", { name: "Cannot reach the server" });
    expect(screen.getByTestId("window-bar").hasAttribute("data-tauri-drag-region")).toBe(true);
    expect(screen.getByRole("button", { name: "Close" })).toBeTruthy();
  });

  it("draw no strip in a browser", async () => {
    vi.stubGlobal(
      "fetch",
      vi.fn(async () => new Response("a token is needed", { status: 401 })),
    );
    mount();
    await screen.findByRole("heading", { name: "Moebius" });
    expect(screen.queryByTestId("window-bar")).toBeNull();
  });
});
