// @vitest-environment jsdom
/** The templates of a repository: which are in effect, from where, and the button that keeps the copies current. */
import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { cleanup, fireEvent, render, screen, waitFor } from "@testing-library/react";
import { afterEach, describe, expect, it, vi } from "vitest";
import { Templates } from "./Templates";

afterEach(() => {
  cleanup();
  vi.unstubAllGlobals();
});

const json = (body: unknown, status = 200) =>
  new Response(JSON.stringify(body), { status, headers: { "content-type": "application/json" } });

const stored = {
  templates: [
    {
      name: "pull_request",
      source: "default",
      path: ".github/PULL_REQUEST_TEMPLATE.md",
      first_line: "",
    },
    { name: "done", source: "project", path: ".github/moebius/done.md", first_line: "/done" },
  ],
  sections: [
    { heading: "Item", required: true, hint: "" },
    { heading: "Notes", required: false, hint: "" },
  ],
  problem: null as string | null,
};

function show(answer: unknown = stored, sync: unknown = { change: { id: 3, pull_request: 12 } }) {
  const fetcher = vi.fn(async (_url: string, init?: RequestInit) =>
    init?.method === "POST" ? json(sync) : json(answer),
  );
  vi.stubGlobal("fetch", fetcher);
  render(
    <QueryClientProvider
      client={new QueryClient({ defaultOptions: { queries: { retry: false } } })}
    >
      <Templates repository={4} />
    </QueryClientProvider>,
  );
  return fetcher;
}

describe("the templates box", () => {
  it("lists each template with where it comes from and where the repository keeps it", async () => {
    show();
    await waitFor(() => expect(screen.getByTestId("template-sections")).toBeTruthy());
    expect(screen.getByTestId("template-sections").textContent).toBe(
      "Pull request sections: Item, Notes (optional)",
    );
    expect(screen.getByText("default, kept at .github/PULL_REQUEST_TEMPLATE.md")).toBeTruthy();
    expect(screen.getByText("the project's, kept at .github/moebius/done.md")).toBeTruthy();
  });

  it("opens the pull request that keeps the copies current, and says it waits", async () => {
    const fetcher = show();
    await waitFor(() => screen.getByRole("button", { name: "Update the repository's copies" }));
    fireEvent.click(screen.getByRole("button", { name: "Update the repository's copies" }));
    await waitFor(() =>
      expect(screen.getByText("Pull request #12: it is in your Inbox.")).toBeTruthy(),
    );
    expect(fetcher).toHaveBeenCalledWith(
      "/v1/repositories/4/templates/sync",
      expect.objectContaining({ method: "POST" }),
    );
    expect(
      screen.getByText("Opens a pull request for .github/. It waits for your click."),
    ).toBeTruthy();
  });

  it("says when the copies are current, and why a project's template is not used", async () => {
    show(
      { ...stored, problem: "templates/pull-request.md is not used: it has no section" },
      { change: null },
    );
    await waitFor(() => screen.getByText(/is not used: it has no section/));
    fireEvent.click(screen.getByRole("button", { name: "Update the repository's copies" }));
    await waitFor(() =>
      expect(screen.getByText("The repository's copies are current.")).toBeTruthy(),
    );
  });
});
