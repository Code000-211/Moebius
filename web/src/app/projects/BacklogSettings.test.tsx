// @vitest-environment jsdom
/** The backlog of a project: where its file is, saved with one button, previewed and synced with the report shown. */
import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { cleanup, fireEvent, render, screen, waitFor, within } from "@testing-library/react";
import { afterEach, describe, expect, it, vi } from "vitest";
import type { Project } from "../../api/types/projects";
import { BacklogSettings } from "./BacklogSettings";

afterEach(() => {
  cleanup();
  vi.unstubAllGlobals();
});

const json = (body: unknown, status = 200) =>
  new Response(JSON.stringify(body), { status, headers: { "content-type": "application/json" } });

const project = {
  id: 3,
  name: "Moebius",
  key: "MOE",
  repositories: [
    { id: 1, name: "moebius", home: true },
    { id: 2, name: "notes", home: false },
  ],
} as unknown as Project;

const stored = {
  project_id: 3,
  repository: "notes",
  path: "notes/backlog/optimisation-backlog.md",
  pattern: "^## (?P<id>[A-Z]-\\d+) (?P<title>.*)$",
  closed_marker: "[closed]",
  cap: 15,
  difficulty_label: "difficulty:",
  lane_label: "lane-required",
};

const report = {
  project: "Moebius",
  repository: "notes",
  path: stored.path,
  dry_run: true,
  items: 3,
  created: [{ id: "K-47", number: 0, title: "K-47: Cache the plan" }],
  updated: [],
  labelled: [],
  closed: [{ id: "P-16", number: 8, title: "P-16: Shorter prompts" }],
  deferred: ["M-01"],
  unchanged: 0,
  warnings: [],
};

function show(backlog: unknown, canEdit = true) {
  const fetcher = vi.fn(async (url: string, init?: RequestInit) => {
    if (init?.method === "PUT")
      return json({ ...stored, ...JSON.parse(String(init.body)), project_id: 3 });
    if (String(url).endsWith("/sync")) return json(report);
    return json(backlog);
  });
  vi.stubGlobal("fetch", fetcher);
  const client = new QueryClient({ defaultOptions: { queries: { retry: false } } });
  render(
    <QueryClientProvider client={client}>
      <BacklogSettings project={project} canEdit={canEdit} />
    </QueryClientProvider>,
  );
  return fetcher;
}

const body = (fetcher: ReturnType<typeof vi.fn>, method: string) =>
  JSON.parse(
    String(
      (fetcher.mock.calls as [string, RequestInit][]).find(
        ([, init]) => init?.method === method,
      )?.[1].body,
    ),
  );

describe("the backlog settings", () => {
  it("asks for the file of a project that has none, and saves it with the defaults for the rest", async () => {
    const fetcher = show(null);
    await waitFor(() => expect(screen.getByLabelText("File")).toBeTruthy());
    const save = screen.getByRole("button", { name: "Save backlog" }) as HTMLButtonElement;
    expect(save.disabled).toBe(true);
    fireEvent.change(screen.getByLabelText("File"), {
      target: { value: "notes/backlog/optimisation-backlog.md" },
    });
    expect((screen.getByLabelText("Most issues one sync makes") as HTMLInputElement).value).toBe(
      "15",
    );
    expect(save.disabled).toBe(false);
    fireEvent.click(save);
    await waitFor(() =>
      expect(body(fetcher, "PUT").path).toBe("notes/backlog/optimisation-backlog.md"),
    );
    expect(body(fetcher, "PUT")).toMatchObject({
      repository: "moebius",
      cap: 15,
      closed_marker: "[closed]",
      difficulty_label: "difficulty:",
      lane_label: "lane-required",
    });
    // No pattern is sent while the field is empty: the default stays.
    expect(body(fetcher, "PUT").pattern).toBeUndefined();
  });

  it("shows what is kept, and does not sync a backlog that has unsaved changes", async () => {
    show(stored);
    await waitFor(() =>
      expect((screen.getByLabelText("File") as HTMLInputElement).value).toBe(stored.path),
    );
    expect((screen.getByRole("button", { name: "Sync now" }) as HTMLButtonElement).disabled).toBe(
      false,
    );
    fireEvent.change(screen.getByLabelText("Most issues one sync makes"), {
      target: { value: "5" },
    });
    expect((screen.getByRole("button", { name: "Sync now" }) as HTMLButtonElement).disabled).toBe(
      true,
    );
    fireEvent.change(screen.getByLabelText("Most issues one sync makes"), {
      target: { value: "500" },
    });
    expect(screen.getByText("A number from 1 to 100.")).toBeTruthy();
  });

  it("previews a sync with a dry run and shows the report", async () => {
    const fetcher = show(stored);
    await waitFor(() =>
      expect(screen.getByRole("button", { name: "Preview a sync" })).toBeTruthy(),
    );
    fireEvent.click(screen.getByRole("button", { name: "Preview a sync" }));
    const shown = await screen.findByTestId("sync-report");
    expect(body(fetcher, "POST")).toEqual({ dry_run: true });
    expect(within(shown).getByText(/A sync would do this/)).toBeTruthy();
    expect(within(shown).getByText("K-47: Cache the plan")).toBeTruthy();
    expect(within(shown).getByText(/#8 P-16: Shorter prompts/)).toBeTruthy();
    expect(within(shown).getByText("M-01")).toBeTruthy();
  });

  it("is read only for anyone who is not an owner", async () => {
    show(stored, false);
    await waitFor(() =>
      expect((screen.getByLabelText("File") as HTMLInputElement).value).toBe(stored.path),
    );
    expect((screen.getByLabelText("File") as HTMLInputElement).disabled).toBe(true);
    expect(screen.queryByRole("button", { name: "Sync now" })).toBeNull();
  });
});
