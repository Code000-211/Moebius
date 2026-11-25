// @vitest-environment jsdom
/** The "Test lane" box: the lane a repository uses, the warning while it is not isolated everywhere, and a paused lane. */
import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { cleanup, fireEvent, render, screen, waitFor } from "@testing-library/react";
import { afterEach, describe, expect, it, vi } from "vitest";
import type { Lane, RepositoryLane } from "../../api/types/lane";
import { TestLane } from "./TestLane";

afterEach(() => {
  cleanup();
  vi.unstubAllGlobals();
});

const json = (body: unknown, status = 200) =>
  new Response(JSON.stringify(body), { status, headers: { "content-type": "application/json" } });

function lane(over: Partial<Lane> = {}): Lane {
  return {
    name: "gpu1",
    runner: "gpu-box",
    isolation: "always",
    warning: null,
    runtime: "podman",
    image: "rocm/dev:6.2",
    gpus: "2x MI300X",
    power_cap: "750 W",
    rocm: "6.2.1",
    driver: null,
    devices: ["/dev/kfd"],
    timeout_seconds: 7200,
    queued: 2,
    running: null,
    paused: null,
    ...over,
  };
}

function view(over: Partial<RepositoryLane> = {}): RepositoryLane {
  const current = lane();
  return {
    lane: "gpu1",
    current,
    lanes: [current],
    definition: { present: true, problem: null, steps: ["build", "measure"], measures: true },
    jobs: [],
    ...over,
  };
}

function show(stored: RepositoryLane) {
  const fetcher = vi.fn(async (_url: string, init?: RequestInit) =>
    init?.method === "PUT"
      ? json({ ...stored, lane: JSON.parse(String(init.body)).lane })
      : json(stored),
  );
  vi.stubGlobal("fetch", fetcher);
  const client = new QueryClient({ defaultOptions: { queries: { retry: false } } });
  render(
    <QueryClientProvider client={client}>
      <TestLane repository={4} />
    </QueryClientProvider>,
  );
  return fetcher;
}

describe("the test lane box", () => {
  it("shows the lane, its machine and what the project's job is, with no warning while it is isolated", async () => {
    const fetcher = show(view());
    await screen.findByTestId("lane-machine");
    expect(fetcher).toHaveBeenCalledWith("/v1/repositories/4/lane", expect.anything());
    const machine = screen.getByTestId("lane-machine").textContent ?? "";
    for (const part of ["gpu-box", "2x MI300X", "750 W", "6.2.1", "rocm/dev:6.2", "2 waiting"]) {
      expect(machine).toContain(part);
    }
    expect(screen.queryByTestId("isolation-warning")).toBeNull();
    expect(screen.getByTestId("lane-definition").textContent).toContain(
      "4 steps".replace("4", "2"),
    );
  });

  it.each(["off", "external_only"] as const)(
    "warns clearly while the isolation is %s",
    async (isolation) => {
      const warning = `Jobs run directly on the machine (${isolation}).`;
      const current = lane({ isolation, warning });
      show(view({ current, lanes: [current] }));
      const shown = await screen.findByTestId("isolation-warning");
      expect(shown.textContent).toContain(warning);
      expect(shown.textContent).toContain(`Isolation is ${isolation.replace("_", " ")}`);
      expect(shown.closest('[role="alert"]')).not.toBeNull();
    },
  );

  it("says why a definition is not valid, and when the server has no lane", async () => {
    show(
      view({
        definition: {
          present: true,
          problem: "lane.toml: there is no [[step]]",
          steps: [],
          measures: false,
        },
      }),
    );
    expect((await screen.findByTestId("lane-definition")).textContent).toContain("is not valid");
    cleanup();
    show(view({ lane: null, current: null, lanes: [] }));
    expect((await screen.findByTestId("no-lanes")).textContent).toContain("[[lanes]]");
  });

  it("puts a lane that paused itself back to work with one button", async () => {
    const current = lane({ paused: { since: 1, reason: "3 jobs in a row failed" } });
    const fetcher = show(view({ current, lanes: [current] }));
    const paused = await screen.findByTestId("lane-paused");
    expect(paused.textContent).toContain("3 jobs in a row failed");
    fireEvent.click(screen.getByRole("button", { name: "Resume the lane" }));
    await waitFor(() =>
      expect(fetcher).toHaveBeenCalledWith(
        "/v1/lanes/gpu1/resume",
        expect.objectContaining({ method: "POST" }),
      ),
    );
  });

  it("lists the latest jobs and what each came to", async () => {
    show(
      view({
        jobs: [
          {
            id: 2,
            state: "failed",
            head: "abcdef0123",
            summary: "the step build failed: it exited with code 2",
          },
          { id: 1, state: "measured", head: "0123456789", summary: "every step passed" },
        ] as RepositoryLane["jobs"],
      }),
    );
    const jobs = (await screen.findByTestId("lane-jobs")).textContent ?? "";
    expect(jobs).toContain("abcdef0");
    expect(jobs).toContain("Failed");
    expect(jobs).toContain("Measured");
  });
});
