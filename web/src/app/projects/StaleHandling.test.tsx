// @vitest-environment jsdom
/** The stale handling of a repository: the days it is read with, and saved with one button. */
import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { cleanup, fireEvent, render, screen, waitFor } from "@testing-library/react";
import { afterEach, describe, expect, it, vi } from "vitest";
import { StaleHandling } from "./StaleHandling";

afterEach(() => {
  cleanup();
  vi.unstubAllGlobals();
});

const json = (body: unknown, status = 200) =>
  new Response(JSON.stringify(body), { status, headers: { "content-type": "application/json" } });

function show(stored = { release_after_days: 14, stale_after_days: 14, close_after_days: 30 }) {
  const fetcher = vi.fn(async (_url: string, init?: RequestInit) =>
    init?.method === "PUT" ? json(JSON.parse(String(init.body))) : json(stored),
  );
  vi.stubGlobal("fetch", fetcher);
  const client = new QueryClient({ defaultOptions: { queries: { retry: false } } });
  render(
    <QueryClientProvider client={client}>
      <StaleHandling repository={4} />
    </QueryClientProvider>,
  );
  return fetcher;
}

const input = (name: string) => screen.getByLabelText(name) as HTMLInputElement;
const save = () => screen.getByRole("button", { name: "Save settings" }) as HTMLButtonElement;

describe("the stale handling box", () => {
  it("shows the defaults of 14, 14 and 30 days, and saves nothing until something changes", async () => {
    const fetcher = show();
    await waitFor(() => expect(input("Release an issue taken for (days)").value).toBe("14"));
    expect(input("Remind after (days)").value).toBe("14");
    expect(input("Close after (days)").value).toBe("30");
    expect(save().disabled).toBe(true);
    expect(fetcher).toHaveBeenCalledWith("/v1/repositories/4/upkeep", expect.anything());
  });

  it("saves the days, 0 turning one off", async () => {
    const fetcher = show();
    await waitFor(() => expect(input("Remind after (days)").value).toBe("14"));
    fireEvent.change(input("Release an issue taken for (days)"), { target: { value: "0" } });
    fireEvent.change(input("Close after (days)"), { target: { value: "60" } });
    expect(save().disabled).toBe(false);
    fireEvent.click(save());
    await waitFor(() => {
      const put = fetcher.mock.calls.find(([, init]) => init?.method === "PUT");
      expect(JSON.parse(String(put?.[1]?.body))).toEqual({
        release_after_days: 0,
        stale_after_days: 14,
        close_after_days: 60,
      });
    });
  });

  it("refuses a close that comes before the reminder, and a number that is no number of days", async () => {
    show();
    await waitFor(() => expect(input("Remind after (days)").value).toBe("14"));
    fireEvent.change(input("Close after (days)"), { target: { value: "10" } });
    expect(screen.getByText("Must be more than the days it is reminded after.")).toBeTruthy();
    expect(save().disabled).toBe(true);
    fireEvent.change(input("Close after (days)"), { target: { value: "0" } });
    expect(save().disabled).toBe(false);
    fireEvent.change(input("Remind after (days)"), { target: { value: "400" } });
    expect(screen.getByText("A number from 0 to 365.")).toBeTruthy();
    expect(save().disabled).toBe(true);
  });
});
