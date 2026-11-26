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
    expect(
      (screen.getByRole("button", { name: "Save settings" }) as HTMLButtonElement).disabled,
    ).toBe(true);
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
    fireEvent.click(screen.getByRole("button", { name: "Save settings" }));
    await waitFor(() => expect(fetcher).toHaveBeenCalled());
    const [url, init] = fetcher.mock.calls[0] as unknown as [string, RequestInit];
    expect([url, init.method]).toEqual(["/v1/repositories/4", "PUT"]);
    expect(sent(fetcher)).toEqual({
      mark_ready: true,
      ready_label: "Go",
      agents_take: "labelled",
      commands: false,
      command_limit: 3,
      answer_on_forge: true,
      merge_reviewed: true,
      require_measurement: false,
      locked_paths: [],
      review_brief: ["REVIEW.md"],
      testing_label: "ready-for-testing",
      measurers: [],
      measurement_free_paths: [],
    });
  });

  it("starts with the commands off, the limit 3 and the answers on, and says whose server turns them on", () => {
    show();
    const part = screen.getByTestId("issue-commands");
    expect(part.textContent).toContain("Commands in issue comments");
    expect(
      screen
        .getByLabelText("Take issues with /take, /release and /done")
        .getAttribute("aria-checked"),
    ).toBe("false");
    expect((screen.getByLabelText("Issues one login may hold") as HTMLInputElement).value).toBe(
      "3",
    );
    expect(screen.getByLabelText("Answer on GitHub").getAttribute("aria-checked")).toBe("true");
    // The hint says it is for the owner's server, and who may use the commands.
    expect(part.textContent).toContain("at the repository owner\u2019s server only");
    expect(part.textContent).toContain("anyone who can comment on the issue");
  });

  it("saves the commands, the limit and the answers, and refuses a limit that is no number from 1 to 50", async () => {
    const fetcher = show();
    fireEvent.click(screen.getByLabelText("Take issues with /take, /release and /done"));
    fireEvent.click(screen.getByLabelText("Answer on GitHub"));
    const limit = screen.getByLabelText("Issues one login may hold");
    fireEvent.change(limit, { target: { value: "0" } });
    expect(screen.getByText("A number from 1 to 50.")).toBeTruthy();
    expect(
      (screen.getByRole("button", { name: "Save settings" }) as HTMLButtonElement).disabled,
    ).toBe(true);
    fireEvent.change(limit, { target: { value: "5" } });
    fireEvent.click(screen.getByRole("button", { name: "Save settings" }));
    await waitFor(() => expect(fetcher).toHaveBeenCalled());
    expect(sent(fetcher)).toEqual({
      mark_ready: false,
      ready_label: "Ready for agent",
      agents_take: "all",
      commands: true,
      command_limit: 5,
      answer_on_forge: false,
      merge_reviewed: true,
      require_measurement: false,
      locked_paths: [],
      review_brief: ["REVIEW.md"],
      testing_label: "ready-for-testing",
      measurers: [],
      measurement_free_paths: [],
    });
  });

  it("names the label in the hint while ready issues are marked", () => {
    show({ mark_ready: true, ready_label: "Go" });
    expect(screen.getByTestId("issue-commands").textContent).toContain('carries "Go"');
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
    fireEvent.click(screen.getByRole("button", { name: "Save settings" }));
    await screen.findByText(/Couldn't save this/);
  });

  it("merges the pull requests the reviewer approves until the owner turns it off, and says what that means", async () => {
    const fetcher = show();
    const merge = screen.getByLabelText("Merge external pull requests the reviewer approves");
    expect(merge.getAttribute("aria-checked")).toBe("true");
    expect(screen.getByText(/Foreign code then lands without a person looking/)).toBeTruthy();
    fireEvent.click(merge);
    fireEvent.click(screen.getByRole("button", { name: "Save settings" }));
    await waitFor(() => expect(fetcher).toHaveBeenCalled());
    expect(sent(fetcher)).toMatchObject({ merge_reviewed: false });
  });

  it("shows a repository that does not merge them as off", () => {
    show({ merge_reviewed: false });
    expect(
      screen
        .getByLabelText("Merge external pull requests the reviewer approves")
        .getAttribute("aria-checked"),
    ).toBe("false");
  });

  it("starts the merging part with the defaults, and says what is always locked", () => {
    show();
    const part = screen.getByTestId("merging");
    expect(part.textContent).toContain("Merging");
    expect(screen.getByLabelText("Require a measurement").getAttribute("aria-checked")).toBe(
      "false",
    );
    expect((screen.getByLabelText("Review brief") as HTMLTextAreaElement).value).toBe("REVIEW.md");
    expect((screen.getByLabelText("Ready for testing") as HTMLInputElement).value).toBe(
      "ready-for-testing",
    );
    expect(part.textContent).toContain(".github/**");
    expect(part.textContent).toContain("AGENTS.md, CLAUDE.md");
  });

  it("shows what the repository has stored and saves the new settings in one request", async () => {
    const fetcher = show({
      require_measurement: true,
      locked_paths: ["kernels/**"],
      review_brief: ["notes/backlog/CARDS.md", "AGENTS.md"],
      testing_label: "needs-bench",
      measurers: ["alice"],
    });
    expect((screen.getByLabelText("Locked paths") as HTMLTextAreaElement).value).toBe("kernels/**");
    expect((screen.getByLabelText("Review brief") as HTMLTextAreaElement).value).toBe(
      "notes/backlog/CARDS.md\nAGENTS.md",
    );
    fireEvent.click(screen.getByLabelText("Require a measurement"));
    fireEvent.change(screen.getByLabelText("Locked paths"), {
      target: { value: "kernels/**\n\n docs/legal/** \nkernels/**" },
    });
    fireEvent.change(screen.getByLabelText("Review brief"), { target: { value: "REVIEW.md" } });
    fireEvent.change(screen.getByLabelText("Ready for testing"), {
      target: { value: " bench " },
    });
    fireEvent.change(screen.getByLabelText("Measurers"), { target: { value: "alice\nbob" } });
    fireEvent.change(screen.getByLabelText("Measurement-free paths"), {
      target: { value: " notes/backlog/closed/** \n\nnotes/backlog/measurements/**" },
    });
    fireEvent.click(screen.getByRole("button", { name: "Save settings" }));
    await waitFor(() => expect(fetcher).toHaveBeenCalled());
    expect(sent(fetcher)).toMatchObject({
      require_measurement: false,
      locked_paths: ["kernels/**", "docs/legal/**"],
      review_brief: ["REVIEW.md"],
      testing_label: "bench",
      measurers: ["alice", "bob"],
      measurement_free_paths: ["notes/backlog/closed/**", "notes/backlog/measurements/**"],
    });
  });

  it("freezes and unfreezes the repository, and says why it is frozen", async () => {
    const fetcher = show();
    fireEvent.click(screen.getByRole("button", { name: "Freeze this repository" }));
    await waitFor(() => expect(fetcher).toHaveBeenCalled());
    const [url, init] = fetcher.mock.calls[0] as unknown as [string, RequestInit];
    expect([url, init.method]).toEqual(["/v1/repositories/4/freeze", "POST"]);
    cleanup();
    const again = show({
      frozen: { reason: "two reverts in 7 days", source: "automatic", since: 1 },
    });
    expect(screen.getByTestId("frozen-notice").textContent).toContain(
      "Frozen: two reverts in 7 days",
    );
    fireEvent.click(screen.getByRole("button", { name: "Unfreeze" }));
    await waitFor(() => expect(again).toHaveBeenCalled());
    expect((again.mock.calls[0] as unknown as [string])[0]).toBe("/v1/repositories/4/unfreeze");
  });

  it("shows the server's refusal to unfreeze while the label is on an issue", async () => {
    show({ frozen: { reason: "the label", source: "label", since: 1 } }, () =>
      json({ error: "remove the label factory:freeze first" }, 409),
    );
    fireEvent.click(screen.getByRole("button", { name: "Unfreeze" }));
    await screen.findByText(/remove the label factory:freeze first/);
  });
});
