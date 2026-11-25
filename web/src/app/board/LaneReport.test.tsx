// @vitest-environment jsdom
/** The lane's report in the card window: a measurement with its rows, a failure with its output, and what waits. */
import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { cleanup, render, screen } from "@testing-library/react";
import { afterEach, describe, expect, it, vi } from "vitest";
import type { CardLane, LaneJob } from "../../api/types/lane";
import { LaneReport } from "./LaneReport";

afterEach(() => {
  cleanup();
  vi.unstubAllGlobals();
});

const HEAD = "d34db33f0123456789abcdef0123456789abcdef";
const BASE = "0123456789abcdef0123456789abcdef01234567";

function job(over: Partial<LaneJob>): LaneJob {
  return {
    id: 1,
    repository_id: 1,
    card_id: 8,
    change_id: 40,
    lane: "gpu1",
    head: HEAD,
    base: BASE,
    external: true,
    state: "measured",
    failed_step: null,
    summary: "every step passed",
    report: null,
    step: null,
    delivered: true,
    queued_at: 1,
    started_at: 2,
    ended_at: 3,
    ...over,
  };
}

function show(data: CardLane) {
  vi.stubGlobal(
    "fetch",
    vi.fn(
      async () =>
        new Response(JSON.stringify(data), { headers: { "content-type": "application/json" } }),
    ),
  );
  const client = new QueryClient({ defaultOptions: { queries: { retry: false } } });
  return render(
    <QueryClientProvider client={client}>
      <LaneReport card={8} />
    </QueryClientProvider>,
  );
}

describe("the lane report of a card", () => {
  it("draws nothing for a card whose repository uses no lane", async () => {
    const view = show({ lane: null, jobs: [], head: null, waiting: null });
    await new Promise((r) => setTimeout(r, 20));
    expect(view.container.querySelector('[data-testid="lane-report"]')).toBeNull();
  });

  it("shows the measurement: the machine, the medians, the change and whether the output stayed the same", async () => {
    show({
      lane: "gpu1",
      head: HEAD,
      waiting: null,
      jobs: [
        job({
          report: {
            machine: {
              lane: "gpu1",
              where: "gpu-box",
              gpus: "2x MI300X",
              power_cap: "750 W",
              rocm: "6.2.1",
              driver: null,
              image: "rocm/dev:6.2",
              isolation: "always",
              contained: true,
            },
            steps: [{ name: "build", state: "passed", seconds: 12 }],
            measurements: [
              {
                step: "measure",
                runs: 3,
                parity: "identical (out/logits.bin, every run)",
                rows: [
                  {
                    name: "decode_tps",
                    unit: "tok/s",
                    before: [40, 41, 42],
                    after: [44, 45, 46],
                    median_before: 41,
                    median_after: 45,
                    change_percent: 9.756,
                  },
                ],
              },
            ],
          },
        }),
      ],
    });
    expect((await screen.findByTestId("lane-state")).textContent).toBe("Measured");
    const machine = screen.getByTestId("lane-job-machine").textContent ?? "";
    expect(machine).toContain("2x MI300X");
    expect(machine).toContain("in a container (rocm/dev:6.2), no network");
    const table = screen.getByTestId("lane-measurement").textContent ?? "";
    for (const part of [
      "decode_tps (tok/s)",
      "41",
      "45",
      "+9.76%",
      "identical",
      "3 runs of each side",
    ]) {
      expect(table).toContain(part);
    }
  });

  it("shows the step a failure stopped at and the end of its output", async () => {
    show({
      lane: "gpu1",
      head: HEAD,
      waiting: null,
      jobs: [
        job({
          state: "failed",
          failed_step: "build",
          report: {
            failure: { step: "build", why: "it exited with code 2", output: "error: no kernel" },
          },
        }),
      ],
    });
    const failure = await screen.findByTestId("lane-failure");
    expect(failure.textContent).toContain("Stopped at build: it exited with code 2");
    expect(failure.textContent).toContain("error: no kernel");
  });

  it("says why nothing is queued, and that a job of an older head counts for nothing", async () => {
    show({
      lane: "gpu1",
      head: "ffff000",
      waiting:
        "the Reviewer has not approved the code of this version yet, and the review comes first",
      jobs: [job({})],
    });
    expect((await screen.findByTestId("lane-waiting")).textContent).toContain(
      "the review comes first",
    );
    expect(screen.getByTestId("lane-old").textContent).toContain("d34db33");
  });

  it("shows a running job's step", async () => {
    show({
      lane: "gpu1",
      head: HEAD,
      waiting: null,
      jobs: [job({ state: "running", step: "measure (after, run 2 of 3)", summary: "" })],
    });
    expect((await screen.findByTestId("lane-job")).textContent).toContain("Running");
    expect(screen.getByTestId("lane-job").textContent).toContain("measure (after, run 2 of 3)");
  });
});
