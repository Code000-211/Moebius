// @vitest-environment jsdom
import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { cleanup, fireEvent, render, screen } from "@testing-library/react";
import { afterEach, describe, expect, it, vi } from "vitest";
import { ToastProvider } from "../../ui/page/Toast";
import { TokensPage } from "./TokensPage";

afterEach(() => {
  cleanup();
  vi.unstubAllGlobals();
});

describe("making a token", () => {
  it("says a refusal as a toast, not as a banner in the page", async () => {
    vi.stubGlobal(
      "fetch",
      vi.fn(async (_url: string, init?: { method?: string }) =>
        init?.method === "POST"
          ? { ok: false, status: 409, text: async () => JSON.stringify({ error: "name in use" }) }
          : { ok: true, status: 200, text: async () => "[]" },
      ),
    );
    const client = new QueryClient({ defaultOptions: { queries: { retry: false } } });
    render(
      <QueryClientProvider client={client}>
        <ToastProvider>
          <TokensPage />
        </ToastProvider>
      </QueryClientProvider>,
    );
    fireEvent.change(screen.getByRole("textbox"), { target: { value: "laptop" } });
    fireEvent.click(screen.getByRole("button", { name: "Make token" }));
    const alert = await screen.findByRole("alert");
    expect(alert.textContent).toContain("Could not make the token.");
    // The toast is drawn by the host, outside the page's own markup.
    expect(screen.getByTestId("tokens-page").contains(alert)).toBe(false);
  });
});
