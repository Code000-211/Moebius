/** The boxes of the Usage page under its tiles: tokens by model, where the model time goes, the longest tool waits, the lens's checks and the usage by agent. */
import type { ReactNode } from "react";
import type { UsageReport } from "../../api/types/usage";
import { cn } from "../../ui/lib/cn";
import { Box } from "../../ui/page/Box";
import { bytes, duration, grouped, milliseconds, share, short } from "./figures";
import { doings, lensWhen, modelColours } from "./model";

/** What a box says when the window has no model call. */
const NO_CALL = "No model call in this window.";

/** A table: bold heads over a line, then a row for each; the first column left, the others right like numbers. */
function Table({ heads, rows }: { heads: string[]; rows: ReactNode[][] }) {
  return (
    <div className="overflow-x-auto">
      <table className="w-full border-collapse text-[13px]">
        <thead>
          <tr className="border-b">
            {heads.map((head, at) => (
              <th
                key={head}
                className={cn(
                  "px-base py-half text-[11px] font-semibold text-low",
                  at === 0 ? "text-left" : "text-right",
                )}
              >
                {head}
              </th>
            ))}
          </tr>
        </thead>
        <tbody>
          {rows.map((row, at) => (
            // biome-ignore lint/suspicious/noArrayIndexKey: the rows are drawn once, in order
            <tr key={at} className="border-b border-track last:border-b-0">
              {row.map((cell, column) => (
                <td
                  // biome-ignore lint/suspicious/noArrayIndexKey: the cells are drawn once, in order
                  key={column}
                  className={cn(
                    "whitespace-nowrap px-base py-[6px] text-high",
                    column === 0 ? "text-left" : "text-right",
                  )}
                >
                  {cell}
                </td>
              ))}
            </tr>
          ))}
        </tbody>
      </table>
    </div>
  );
}

const code = (text: string) => <span className="font-mono text-[12px]">{text}</span>;

/** The stacked bar of the models' shares, and the table of what each used. */
export function ModelsBox({ report, span }: { report: UsageReport; span: string }) {
  const colour = modelColours(report);
  const total = report.models.reduce((sum, model) => sum + model.tokens, 0);
  return (
    <Box
      title="Tokens by model"
      aside={`${short(total)} ${span} across ${report.models.length} models`}
    >
      {report.models.length === 0 ? (
        <p className="m-0 text-[13px] text-normal">{NO_CALL}</p>
      ) : (
        <div className="flex flex-col gap-[12px]">
          <div className="flex gap-[2px]" aria-hidden="true">
            {report.models.map((model) => (
              <span
                key={model.model}
                className="h-[12px] first:rounded-l-md last:rounded-r-md"
                style={{
                  flex: `${Math.max(1, Math.floor((Math.max(0, model.tokens) * 1000) / Math.max(1, total)))} 1 0`,
                  background: `var(--model-${colour(model.model)})`,
                }}
              />
            ))}
          </div>
          <Table
            heads={["Model", "Tokens", "Share", "Calls", "Tokens per call", "From cache"]}
            rows={report.models.map((model) => [
              <span key="name" className="inline-flex items-center gap-base">
                <span
                  className="size-[10px] rounded-[2px]"
                  style={{ background: `var(--model-${colour(model.model)})` }}
                />
                {model.model}
              </span>,
              short(model.tokens),
              share(model.tokens, total),
              grouped(model.calls),
              grouped(Math.floor(model.tokens / Math.max(1, model.calls))),
              share(model.cached_tokens, model.input_tokens),
            ])}
          />
        </div>
      )}
    </Box>
  );
}

/** What the model did with its time, as a bar for each activity against the one that took longest. */
export function TimeBox({ report, span }: { report: UsageReport; span: string }) {
  const list = doings(report.activities);
  const longest = Math.max(1, ...list.map((each) => each.modelMs));
  return (
    <Box
      title="Where the model time goes"
      aside={`${duration(report.totals.model_ms / 1000)} ${span}`}
    >
      {list.length === 0 ? (
        <p className="m-0 text-[13px] text-normal">{NO_CALL}</p>
      ) : (
        <ul className="m-0 flex list-none flex-col gap-[12px] p-0">
          {list.map((each) => (
            <li key={each.name} className="flex items-center gap-[12px]">
              <span className="w-[180px] shrink-0 truncate text-[13px] text-high">
                {each.tool ? code(each.name) : each.name}
                <span className="font-mono text-[11px] text-low">
                  {` · ${grouped(each.calls)} ${each.calls === 1 ? "call" : "calls"}`}
                </span>
              </span>
              <span className="h-[12px] flex-1 rounded-r-md bg-[var(--head-pill)]">
                <span
                  className="block h-full rounded-r-md"
                  style={{
                    width: `${Math.max(0.1, (each.modelMs * 100) / longest)}%`,
                    background: "var(--model-1)",
                  }}
                />
              </span>
              <span className="w-[80px] shrink-0 text-right font-mono text-[12.5px] text-normal">
                {duration(each.modelMs / 1000)}
              </span>
            </li>
          ))}
        </ul>
      )}
    </Box>
  );
}

/** The tools whose calls made runs wait longest, and what making the tools' outputs smaller saved. */
export function WaitsBox({ report }: { report: UsageReport }) {
  const saving = report.tool_output;
  return (
    <Box
      title="Longest tool waits"
      aside={
        saving && saving.results > 0
          ? `Outputs cut from ${bytes(saving.bytes_before)} to ${bytes(saving.bytes_after)}`
          : undefined
      }
    >
      {report.tool_waits.length === 0 ? (
        <p className="m-0 text-[13px] text-normal">No tool call waited in this window.</p>
      ) : (
        <Table
          heads={["Tool call", "Calls", "Waited"]}
          rows={report.tool_waits.map((wait) => [
            code(wait.tool),
            grouped(wait.calls),
            duration(wait.seconds),
          ])}
        />
      )}
    </Box>
  );
}

/** Where the time of the lint and type checks after the agents' edits went, by phase and tool; nothing when there was none. */
export function LensBox({ report }: { report: UsageReport }) {
  const lens = report.lens ?? [];
  if (lens.length === 0) return null;
  return (
    <Box
      title="Lint and type checks"
      aside={milliseconds(lens.reduce((sum, check) => sum + check.ms, 0))}
    >
      <Table
        heads={["Check", "When", "Runs", "Skipped", "Time", "Told to the agent"]}
        rows={lens.map((check) => [
          code(check.tool),
          lensWhen(check.phase),
          grouped(check.runs),
          grouped(check.skipped),
          milliseconds(check.ms),
          grouped(check.shown),
        ])}
      />
    </Box>
  );
}

export function AgentsBox({ report }: { report: UsageReport }) {
  return (
    <Box title="By agent">
      {report.agents.length === 0 ? (
        <p className="m-0 text-[13px] text-normal">{NO_CALL}</p>
      ) : (
        <Table
          heads={["Agent", "Calls", "Tokens", "From cache", "Model time"]}
          rows={report.agents.map((agent) => [
            agent.agent,
            grouped(agent.calls),
            short(agent.tokens),
            share(agent.cached_tokens, agent.input_tokens),
            duration(agent.model_ms / 1000),
          ])}
        />
      )}
    </Box>
  );
}
