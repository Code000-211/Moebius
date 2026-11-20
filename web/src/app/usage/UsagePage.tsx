/**
 * The Usage page (the former desktop app's `screens/usage`): what the model calls used, from `GET /v1/usage`. The page leads
 * with what today cost against the day's budget, because that is the question the page is opened to answer. A window
 * (today, 7 or 30 days) sets the figures under it, the models, what the model did with its time, the agents and the
 * tool waits; the heatmap always shows the last year. The tokens used today also feed the meter at the foot of the
 * sidebar. The shared filter bar narrows every figure to the runs of one project and one agent (`../scope.ts`), which
 * the server does (`GET /v1/usage?repository=&agent=`); the filters are in the address.
 */
import { useQuery } from "@tanstack/react-query";
import { usage } from "../../api/calls/usage";
import { sayError } from "../../api/errorText";
import { keys } from "../../api/keys";
import type { UsageReport } from "../../api/types/usage";
import { Button } from "../../ui/components/Button";
import { FilterBar } from "../../ui/filters/FilterBar";
import { cn } from "../../ui/lib/cn";
import { Chip } from "../../ui/page/Chip";
import { Figures } from "../../ui/page/Figures";
import { Notice } from "../../ui/page/Notice";
import { PageHeader } from "../../ui/page/PageHeader";
import { Reading, SkeletonLines } from "../../ui/page/Skeleton";
import { useDays, useScopeFilters } from "../scope";
import { AgentsBox, ModelsBox, TimeBox, WaitsBox } from "./Boxes";
import { Heatmap } from "./Heatmap";
import { type MeterTone, tiles, tokenFigure, WINDOWS } from "./model";

/** The hue of the bar for how today stands against its budget. The word comes with the pill. */
const METER: Record<MeterTone, string> = {
  progress: "bg-progress",
  attention: "bg-attention",
  error: "bg-error",
};

/**
 * The figures at the head of the page: the tokens the chosen window used, then its other totals, with today against
 * the day's budget as one thin bar and a line under them.
 */
function Head({ report }: { report: UsageReport }) {
  const { title, value, note, meter } = tokenFigure(report);
  return (
    <section className="flex flex-col gap-3 pb-4">
      <Figures
        items={[
          { label: title, value },
          ...tiles(report).map((tile) => ({
            label: tile.label,
            value: tile.value,
            hint: tile.note,
          })),
        ]}
      />
      <div className="flex flex-wrap items-center gap-3 text-sm text-muted">
        {meter && <span className="text-text">{meter.word}</span>}
        {meter && (
          <div
            aria-hidden="true"
            className="flex h-1.5 w-64 gap-px overflow-hidden rounded-full bg-track"
          >
            <span
              className={cn("h-full rounded-full", METER[meter.tone])}
              style={{ width: `${Math.min(100, meter.used * 100)}%` }}
            />
          </div>
        )}
        <span>{note}</span>
      </div>
    </section>
  );
}

export function UsagePage() {
  const [days, setDays] = useDays(
    WINDOWS.map((window) => window.days),
    1,
  );
  const filters = useScopeFilters();
  const { repository = null, agent = "" } = filters.scope;
  const report = useQuery({
    queryKey: keys.usageDays(days, repository, agent),
    queryFn: () => usage(days, filters.scope),
    // Keeps the figures of the window before while the chosen one is read.
    placeholderData: (previous) => previous,
  });
  const data = report.data;
  return (
    <>
      <PageHeader title="Usage" />
      <FilterBar
        label="Filter the usage"
        leading={
          <fieldset className="m-0 flex flex-wrap gap-2 border-0 p-0">
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
        }
        fields={filters.fields}
        values={filters.values}
        onFilter={filters.set}
        onClear={filters.clear}
      />
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
        <Reading message="Reading usage" className="flex flex-col gap-4">
          <SkeletonLines lines={2} />
        </Reading>
      ) : (
        <div className="flex flex-col gap-4" data-window={data.days_in_window}>
          <Head report={data} />
          <Heatmap report={data} />
          <ModelsBox report={data} />
          <div className="grid gap-4 lg:grid-cols-2">
            <TimeBox report={data} />
            <WaitsBox report={data} />
          </div>
          <AgentsBox report={data} />
        </div>
      )}
    </>
  );
}
