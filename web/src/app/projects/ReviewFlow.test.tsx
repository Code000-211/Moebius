// @vitest-environment jsdom
/** The review flow of a repository: the settings it is read with, and saved with one button. */
import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { cleanup, fireEvent, render, screen, waitFor } from "@testing-library/react";
import { afterEach, describe, expect, it, vi } from "vitest";
import { ReviewFlow } from "./ReviewFlow";

afterEach(() => {
  cleanup();
  vi.unstubAllGlobals();
});

const json = (body: unknown, status = 200) =>
  new Response(JSON.stringify(body), { status, headers: { "content-type": "application/json" } });

function show(
  stored = {
    review_on_green: false,
    green_wait_minutes: 5,
    unbound_close_days: 14,
    status_labels: false,
    status_prefix: "status:",
  },
) {
  const fetcher = vi.fn(async (_url: string, init?: RequestInit) =>
    init?.method === "PUT" ? json(JSON.parse(String(init.body))) : json(stored),
  );
  vi.stubGlobal("fetch", fetcher);
  const client = new QueryClient({ defaultOptions: { queries: { retry: false } } });
  render(
    <QueryClientProvider client={client}>
      <ReviewFlow repository={4} />
    </QueryClientProvider>,
  );
  return fetcher;
}

const input = (name: string) => screen.getByLabelText(name) as HTMLInputElement;
const save = () => screen.getByRole("button", { name: "Save settings" }) as HTMLButtonElement;
const green = () => screen.getByLabelText("Review when the checks are green");

describe("the review flow box", () => {
  it("is off by default, shows 5 minutes and 14 days, and saves nothing until something changes", async () => {
    const fetcher = show();
    await waitFor(() => expect(input("Green for (minutes)").value).toBe("5"));
    expect(green().getAttribute("aria-checked")).toBe("false");
    expect(input("Close a pull request that names no issue after (days)").value).toBe("14");
    expect(save().disabled).toBe(true);
    expect(fetcher).toHaveBeenCalledWith("/v1/repositories/4/flow", expect.anything());
  });

  it("saves the switch and the numbers together", async () => {
    const fetcher = show();
    await waitFor(() => expect(input("Green for (minutes)").value).toBe("5"));
    fireEvent.click(green());
    fireEvent.change(input("Green for (minutes)"), { target: { value: "10" } });
    fireEvent.change(input("Close a pull request that names no issue after (days)"), {
      target: { value: "0" },
    });
    expect(save().disabled).toBe(false);
    fireEvent.click(save());
    await waitFor(() => {
      const put = fetcher.mock.calls.find(([, init]) => init?.method === "PUT");
      expect(JSON.parse(String(put?.[1]?.body))).toEqual({
        review_on_green: true,
        green_wait_minutes: 10,
        unbound_close_days: 0,
        status_labels: false,
        status_prefix: "status:",
      });
    });
  });

  it("saves the status labels switch with its prefix, and refuses an empty prefix", async () => {
    const fetcher = show();
    await waitFor(() => expect(input("Status label prefix").value).toBe("status:"));
    fireEvent.click(screen.getByLabelText("Status labels on the forge"));
    fireEvent.change(input("Status label prefix"), { target: { value: "" } });
    expect(save().disabled).toBe(true);
    fireEvent.change(input("Status label prefix"), { target: { value: "state/" } });
    fireEvent.click(save());
    await waitFor(() => {
      const put = fetcher.mock.calls.find(([, init]) => init?.method === "PUT");
      const body = JSON.parse(String(put?.[1]?.body));
      expect([body.status_labels, body.status_prefix]).toEqual([true, "state/"]);
    });
  });

  it("refuses a number that is no number of minutes or days, and keeps the button off", async () => {
    show();
    await waitFor(() => expect(input("Green for (minutes)").value).toBe("5"));
    fireEvent.change(input("Green for (minutes)"), { target: { value: "500" } });
    expect(screen.getByText("A number from 0 to 120.")).toBeTruthy();
    expect(save().disabled).toBe(true);
    fireEvent.change(input("Green for (minutes)"), { target: { value: "5" } });
    fireEvent.change(input("Close a pull request that names no issue after (days)"), {
      target: { value: "1.5" },
    });
    expect(screen.getByText("A number from 0 to 365.")).toBeTruthy();
    expect(save().disabled).toBe(true);
  });

  it("says when it could not be read", async () => {
    vi.stubGlobal(
      "fetch",
      vi.fn(async () => json({ message: "no" }, 500)),
    );
    const client = new QueryClient({ defaultOptions: { queries: { retry: false } } });
    render(
      <QueryClientProvider client={client}>
        <ReviewFlow repository={4} />
      </QueryClientProvider>,
    );
    await waitFor(() => expect(screen.getByText(/could not be read/)).toBeTruthy());
  });
});
