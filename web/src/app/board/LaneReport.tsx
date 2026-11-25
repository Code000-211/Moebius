/**
 * What the test lane (ADR 0035) did for a card, in the card window: the latest job for the head the change has now (queued,
 * running at a step, measured, passed, failed at a step or errored), the machine it ran on and how it was isolated, the
 * steps, the measurement with its rows before and after and whether the output stayed the same, and, for a failure, the
 * step and the end of its output. A job of an older head is told in one line, since a push made it count for nothing.
 * Nothing is drawn for a card whose repository uses no lane and has no job.
 */
import { useQuery } from "@tanstack/react-query";
import { readCardLane } from "../../api/calls/lane";
import { keys } from "../../api/keys";
import type { LaneJob } from "../../api/types/lane";
import { JOB_STATE } from "../projects/TestLane";

const short = (commit: string) => commit.slice(0, 7);

function number(value: number | null): string {
  if (value === null) return "-";
  return Number.isInteger(value) ? String(value) : String(Number(value.toFixed(4)));
}

export function LaneReport({ card }: { card: number }) {
  const report = useQuery({
    queryKey: keys.cardLane(card),
    queryFn: () => readCardLane(card),
  });
  const data = report.data;
  if (!data?.jobs || (!data.lane && data.jobs.length === 0)) return null;
  const current = data.jobs.filter((job) => job.head === data.head).at(-1);
  const old = data.jobs.filter((job) => job.head !== data.head);
  return (
    <section
      className="flex min-w-0 flex-col gap-3 px-6 pb-4"
      data-testid="lane-report"
      aria-label="Test lane"
    >
      <h3 className="m-0 text-base font-semibold text-text">
        {data.lane ? `Test lane ${data.lane}` : "Test lane"}
      </h3>
      {current ? (
        <Job job={current} />
      ) : (
        data.waiting && (
          <p className="m-0 text-base text-secondary" data-testid="lane-waiting">
            {`No job is queued: ${data.waiting}.`}
          </p>
        )
      )}
      {old.length > 0 && (
        <p className="m-0 text-sm text-muted" data-testid="lane-old">
          {`A job for ${old.map((job) => short(job.head)).join(", ")} counts for nothing: the pull request has a new commit.`}
        </p>
      )}
    </section>
  );
}

function Job({ job }: { job: LaneJob }) {
  const report = job.report;
  const machine = report?.machine;
  const running = job.state === "running";
  return (
    <div className="flex min-w-0 flex-col gap-3" data-testid="lane-job">
      <p className="m-0 flex flex-wrap items-baseline gap-x-3 text-base text-text">
        <span className="font-semibold" data-testid="lane-state">
          {JOB_STATE[job.state]}
        </span>
        <span className="font-mono text-sm text-secondary">{`${short(job.base)}..${short(job.head)}`}</span>
        {running && job.step && <span className="text-sm text-muted">{job.step}</span>}
        {!running && job.summary && (
          <span className="min-w-0 text-sm text-muted [overflow-wrap:anywhere]">{job.summary}</span>
        )}
      </p>
      {machine && (
        <p className="m-0 text-sm text-secondary" data-testid="lane-job-machine">
          {[
            machine.gpus,
            machine.power_cap && `${machine.power_cap} cap`,
            machine.rocm && `ROCm ${machine.rocm}`,
            machine.driver && `driver ${machine.driver}`,
            machine.contained
              ? `in a container (${machine.image ?? "image"}), no network`
              : "directly on the machine",
          ]
            .filter(Boolean)
            .join(" · ")}
        </p>
      )}
      {report?.steps && report.steps.length > 0 && (
        <ul className="m-0 flex list-none flex-wrap gap-x-4 gap-y-1 p-0 text-sm text-secondary">
          {report.steps.map((step) => (
            <li key={step.name}>{`${step.name}: ${step.state}, ${step.seconds} s`}</li>
          ))}
        </ul>
      )}
      {report?.measurements?.map((m) => (
        <div key={m.step} className="flex min-w-0 flex-col gap-1" data-testid="lane-measurement">
          <span className="text-sm font-semibold text-secondary">
            {`${m.step}: ${m.runs} runs of each side, before and after by turns`}
          </span>
          <div className="min-w-0 overflow-x-auto">
            <table className="w-full border-collapse text-sm">
              <thead>
                <tr className="text-left text-secondary">
                  <th className="py-1 pr-4 font-semibold">Row</th>
                  <th className="py-1 pr-4 text-right font-semibold">Before</th>
                  <th className="py-1 pr-4 text-right font-semibold">After</th>
                  <th className="py-1 text-right font-semibold">Change</th>
                </tr>
              </thead>
              <tbody>
                {m.rows.map((row) => (
                  <tr key={row.name} className="border-t border-hairline text-text">
                    <td className="py-1 pr-4">
                      {row.unit ? `${row.name} (${row.unit})` : row.name}
                    </td>
                    <td className="py-1 pr-4 text-right tabular-nums">
                      {number(row.median_before)}
                    </td>
                    <td className="py-1 pr-4 text-right tabular-nums">
                      {number(row.median_after)}
                    </td>
                    <td className="py-1 text-right tabular-nums">
                      {row.change_percent === null
                        ? "-"
                        : `${row.change_percent > 0 ? "+" : ""}${row.change_percent.toFixed(2)}%`}
                    </td>
                  </tr>
                ))}
              </tbody>
            </table>
          </div>
          <span className="text-sm text-muted">{`Medians. Output parity: ${m.parity}`}</span>
        </div>
      ))}
      {report?.failure && (
        <div className="flex min-w-0 flex-col gap-1" data-testid="lane-failure">
          <span className="text-sm font-semibold text-error-text">
            {`Stopped at ${report.failure.step}: ${report.failure.why}`}
          </span>
          {report.failure.output.trim() && (
            <pre className="m-0 max-h-48 overflow-auto rounded-control bg-sunken p-3 font-mono text-xs text-secondary">
              {report.failure.output}
            </pre>
          )}
        </div>
      )}
      {report?.error && (
        <p className="m-0 text-sm text-error-text" data-testid="lane-error">
          {report.error}
        </p>
      )}
    </div>
  );
}
