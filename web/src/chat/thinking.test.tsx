// @vitest-environment jsdom
import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { cleanup, render, screen, waitFor } from "@testing-library/react";
import { afterEach, describe, expect, it, vi } from "vitest";
import { ThinkingSetting, useThinkingExpanded } from "./thinking";

afterEach(() => {
  cleanup();
  vi.unstubAllGlobals();
});

function Reader() {
  return <p data-testid="value">{String(useThinkingExpanded())}</p>;
}

function mount() {
  const client = new QueryClient({ defaultOptions: { queries: { retry: false } } });
  render(
    <QueryClientProvider client={client}>
      <ThinkingSetting>
        <Reader />
      </ThinkingSetting>
    </QueryClientProvider>,
  );
}

describe("the thinking setting", () => {
  it("is off without a provider", () => {
    render(<Reader />);
    expect(screen.getByTestId("value").textContent).toBe("false");
  });

  it("is read from the person's record on the server, not from browser storage", async () => {
    localStorage.setItem("show_thinking_expanded", "false");
    const fetched = vi.fn(
      async () => new Response(JSON.stringify({ show_thinking_expanded: true })),
    );
    vi.stubGlobal("fetch", fetched);
    mount();
    await waitFor(() => expect(screen.getByTestId("value").textContent).toBe("true"));
    expect(String((fetched.mock.calls as unknown[][])[0]?.[0])).toContain("/v1/preferences");
    localStorage.clear();
  });

  it("stays off while the record has not come", () => {
    vi.stubGlobal("fetch", () => new Promise(() => {}));
    mount();
    expect(screen.getByTestId("value").textContent).toBe("false");
  });
});
