// @vitest-environment jsdom
/**
 * The headline of a change on a card. An empty diff is an answer, not a failed read: a change whose branch was merged
 * or deleted has nothing left to show, and a "try again" for it could never succeed.
 */
import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { cleanup, fireEvent, render, screen } from "@testing-library/react";
import { afterEach, describe, expect, it, vi } from "vitest";

const api = vi.hoisted(() => ({ diff: "" }));
vi.mock("../api/calls/runs", () => ({
  changeDiff: async () => ({ diff: api.diff, truncated: false }),
  runDiff: async () => ({ diff: api.diff, truncated: false }),
}));

const { ChangeDelta, ChangesCard } = await import("./Changes");

afterEach(cleanup);

function show(worktree?: string) {
  render(
    <QueryClientProvider
      client={new QueryClient({ defaultOptions: { queries: { retry: false } } })}
    >
      <ChangesCard of={{ change: 3 }} worktree={worktree} />
    </QueryClientProvider>,
  );
}

describe("the changes of a card", () => {
  it("says there are no changes for an empty diff, without a failed read or a retry", async () => {
    api.diff = "";
    show();
    expect(await screen.findByText("No changes")).toBeTruthy();
    expect(screen.queryByText(/Couldn't read the changes/)).toBeNull();
    expect(screen.queryByText("Try again")).toBeNull();
  });

  it("offers the diff when there are files", async () => {
    api.diff = "diff --git a/a.txt b/a.txt\n--- a/a.txt\n+++ b/a.txt\n@@ -1,1 +1,1 @@\n-a\n+b\n";
    show();
    expect(await screen.findByTestId("diff-card")).toBeTruthy();
  });

  it("names the worktree the change was made in", async () => {
    api.diff = "diff --git a/a.txt b/a.txt\n--- a/a.txt\n+++ b/a.txt\n@@ -1,1 +1,1 @@\n-a\n+b\n";
    show("/srv/wt/moebius/run-28");
    expect(await screen.findByText("run-28")).toBeTruthy();
  });

  it("names no worktree when none is given", async () => {
    api.diff = "diff --git a/a.txt b/a.txt\n--- a/a.txt\n+++ b/a.txt\n@@ -1,1 +1,1 @@\n-a\n+b\n";
    show();
    await screen.findByTestId("diff-card");
    expect(screen.queryByText("run-28")).toBeNull();
  });
});

describe("the size of a change in a card's title row", () => {
  it("is a button with the plus and the minus apart, and a click opens the diff", async () => {
    api.diff =
      "diff --git a/a.txt b/a.txt\n--- a/a.txt\n+++ b/a.txt\n@@ -1,1 +1,2 @@\n-a\n+b\n+c\n";
    render(
      <QueryClientProvider
        client={new QueryClient({ defaultOptions: { queries: { retry: false } } })}
      >
        <ChangeDelta change={3} />
      </QueryClientProvider>,
    );
    const button = await screen.findByRole("button", { name: "Open the changes" });
    expect(button.textContent).toContain("+2");
    expect(button.querySelector(".text-running-text")?.textContent).toContain("+2");
    expect(button.querySelector(".text-error-text")?.textContent).toContain("-1");
    fireEvent.click(button);
    expect(await screen.findByTestId("change-diff")).toBeTruthy();
  });
});
