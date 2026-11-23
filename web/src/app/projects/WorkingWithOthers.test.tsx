// @vitest-environment jsdom
/** The "Working with others" box: the ready label, which issues the agents take, and saving them with one button. */
import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { cleanup, fireEvent, render, screen, waitFor } from "@testing-library/react";
import { afterEach, describe, expect, it, vi } from "vitest";
import type { Repository } from "../../api/types/board";
import { WorkingWithOthers } from "./WorkingWithOthers";

afterEach(() => {
  cleanup();
  vi.unstubAllGlobals();
});

const repository: Repository = {
  id: 4,
  name: "widgets",
  path: "/r",
  forge: "github",
  default_branch: "main",
  github: "acme/widgets",
  trusted: [],
  home_project_id: 1,
};

function show(over: Partial<Repository> = {}, answer: () => Response = () => json(repository)) {
  const fetcher = vi.fn(async () => answer());
  vi.stubGlobal("fetch", fetcher);
  const client = new QueryClient({ defaultOptions: { queries: { retry: false } } });
  render(
    <QueryClientProvider client={client}>
      <WorkingWithOthers repository={{ ...repository, ...over }} />
    </QueryClientProvider>,
  );
  return fetcher;
}

const json = (body: unknown, status = 200) =>
  new Response(JSON.stringify(body), { status, headers: { "content-type": "application/json" } });

/** The body of the last request. */
const sent = (fetcher: ReturnType<typeof vi.fn>) =>
  JSON.parse(String((fetcher.mock.calls.at(-1) as unknown as [string, RequestInit])[1].body));

describe("the repository's working with others", () => {
  it("starts with the defaults of a repository nobody set up: nothing marked, the agents take every issue", () => {
    show();
    expect(
      (screen.getByLabelText("Mark ready issues on GitHub") as HTMLElement).getAttribute(
        "aria-checked",
      ),
    ).toBe("false");
    expect((screen.getByLabelText("Label") as HTMLInputElement).value).toBe("Ready for agent");
    expect(screen.getByRole("button", { name: "Agents take" }).textContent).toContain(
      "All approved issues",
    );
    expect((screen.getByRole("button", { name: "Save" }) as HTMLButtonElement).disabled).toBe(true);
    // The hint says whose server should mark the issues.
    expect(screen.getByText(/Only the server of the person who owns the repository/)).toBeTruthy();
  });

  it("saves the switch, the label and what the agents take in one request", async () => {
    const fetcher = show();
    fireEvent.click(screen.getByLabelText("Mark ready issues on GitHub"));
    fireEvent.change(screen.getByLabelText("Label"), { target: { value: " Go " } });
    fireEvent.pointerDown(screen.getByRole("button", { name: "Agents take" }), {
      button: 0,
      ctrlKey: false,
    });
    fireEvent.click(
      await screen.findByRole("menuitemradio", {
        name: /Only issues labelled "Go" and not assigned/,
      }),
    );
    fireEvent.click(screen.getByRole("button", { name: "Save" }));
    await waitFor(() => expect(fetcher).toHaveBeenCalled());
    const [url, init] = fetcher.mock.calls[0] as unknown as [string, RequestInit];
    expect([url, init.method]).toEqual(["/v1/repositories/4", "PUT"]);
    expect(sent(fetcher)).toEqual({ mark_ready: true, ready_label: "Go", agents_take: "labelled" });
  });

  it("shows what the repository has, and says the server's reason when it refuses", async () => {
    show({ mark_ready: true, ready_label: "Go", agents_take: "labelled" }, () =>
      json({ error: '"x" is not a label name' }, 422),
    );
    expect(screen.getByLabelText("Mark ready issues on GitHub").getAttribute("aria-checked")).toBe(
      "true",
    );
    expect((screen.getByLabelText("Label") as HTMLInputElement).value).toBe("Go");
    fireEvent.change(screen.getByLabelText("Label"), { target: { value: "x" } });
    fireEvent.click(screen.getByRole("button", { name: "Save" }));
    await screen.findByText(/Couldn't save this/);
  });
});
