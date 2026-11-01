/**
 * The Usage page (the desktop app's `screens/usage`): what the model calls used, from `GET /v1/usage`. The page leads
 * with what today cost against the day's budget, because that is the question the page is opened to answer. A window
 * (today, 7 or 30 days) sets the figures under it, the models, what the model did with its time, the agents and the
 * tool waits; the heatmap always shows the last year. The tokens used today also feed the meter at the foot of the
 * sidebar.
 */
import { useQuery } from "@tanstack/react-query";
import { useState } from "react";
import { usage } from "../../api/calls/usage";
import { sayError } from "../../api/errorText";
import { keys } from "../../api/keys";
import type { UsageReport } from "../../api/types/usage";
import { Button } from "../../ui/components/Button";
import { Loader } from "../../ui/components/Loader";
import { cn } from "../../ui/lib/cn";
import { Box } from "../../ui/page/Box";
import { Chip } from "../../ui/page/Chip";
import { Notice } from "../../ui/page/Notice";
import { PageHeader } from "../../ui/page/PageHeader";
import { Status } from "../../ui/page/Status";
import { AgentsBox, LensBox, ModelsBox, TimeBox, WaitsBox } from "./Boxes";
import { short } from "./figures";
import { Heatmap } from "./Heatmap";
import { type MeterTone, span, tiles, tokenFigure, WINDOWS } from "./model";

/** The hue of the bar for how today stands against its budget. The word comes with the pill. */
const METER: Record<MeterTone, string> = {
  progress: "bg-progress",
  attention: "bg-attention",
  error: "bg-error",
};

/** The focal figure: what the chosen window cost, and today against the day's budget. */
function WindowBox({ report }: { report: UsageReport }) {
  const { title, value, note, meter } = tokenFigure(report);
  return (
    <section className="flex flex-col gap-4 rounded-card bg-surface p-4 shadow-card">
      <div className="flex flex-wrap items-center gap-3">
        <h2 className="m-0 text-lg font-semibold tracking-tight text-text">{title}</h2>
        {meter && <Status tone={meter.tone}>{meter.word}</Status>}
      </div>
      <div className="flex flex-wrap items-baseline gap-3">
        <span className="text-2xl font-semibold tabular-nums text-text">{value}</span>
        <span className="text-base text-secondary">{note}</span>
      </div>
      {meter && (
        <>
          <div aria-hidden="true" className="flex h-2 gap-px overflow-hidden rounded-full bg-track">
            {meter.used > 0 && (
              <span
                className={cn("h-full rounded-full", METER[meter.tone])}
                style={{ flex: `${Math.min(100, meter.used) * 100} 1 0` }}
              />
            )}
            <span className="h-full flex-1 rounded-full bg-track" />
          </div>
          <p className="m-0 text-sm text-secondary">
            {meter.used >= 1 ? (
              <>
                Today is over its budget by{" "}
                <span className="tabular-nums">{short(meter.tokens - meter.budget)}</span>.
              </>
            ) : (
              <>
                <span className="tabular-nums">{short(meter.budget - meter.tokens)}</span> left for
                today.
              </>
            )}
          </p>
        </>
      )}
    </section>
  );
}

export function UsagePage() {
  const [days, setDays] = useState(1);
  const report = useQuery({
    queryKey: keys.usageDays(days),
    queryFn: () => usage(days),
    // Keeps the figures of the window before while the chosen one is read.
    placeholderData: (previous) => previous,
  });
  const data = report.data;
  return (
    <>
      <PageHeader title="Usage" />
      <fieldset className="m-0 mb-4 flex flex-wrap gap-2 border-0 p-0">
        <legend className="sr-only">Window</legend>
        {WINDOWS.map((window) => (
          <Chip
            key={window.days}
            selected={window.days === days}
            onClick={() => setDays(window.days)}
          >
            {window.label}
          </Chip>
        ))}
      </fieldset>
      {report.isError && !data ? (
        <Notice
          tone="error"
          action={
            <Button
              size="sm"
              variant="secondary"
              disabled={report.isFetching}
              onClick={() => void report.refetch()}
            >
              Try again
            </Button>
          }
        >
          {sayError(report.error, "Could not read the usage. Try again.")}
        </Notice>
      ) : !data ? (
        <Loader message="Reading usage" className="py-8" />
      ) : (
        <div className="flex flex-col gap-4" data-window={data.days_in_window}>
          <WindowBox report={data} />
          {/* The rest of the window's totals; the tokens it used are the figure above. */}
          <div className="grid gap-4 sm:grid-cols-3">
            {tiles(data).map((tile) => (
              <Box key={tile.label} className="flex flex-col gap-2">
                <span className="text-sm font-semibold text-secondary">{tile.label}</span>
                <span className="text-xl font-semibold tabular-nums text-text">{tile.value}</span>
                <span className="text-sm text-secondary">{tile.note}</span>
              </Box>
            ))}
          </div>
          <Heatmap report={data} />
          <ModelsBox report={data} span={span(data.days_in_window)} />
          <div className="grid gap-4 lg:grid-cols-2">
            <TimeBox report={data} span={span(data.days_in_window)} />
            <WaitsBox report={data} />
          </div>
          <LensBox report={data} />
          <AgentsBox report={data} />
        </div>
      )}
    </>
  );
}
