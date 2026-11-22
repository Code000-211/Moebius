/**
 * The Analytics page: what the moves and runs the server keeps say about the work. The page leads with what was done in
 * the window and how long cards take, then the cards done each day, the time cards wait in each column, what each
 * project spent per point, and the burn-up of the latest cycles and milestones. A window (7, 30 or 90 days) sets
 * the done cards and moves counted; the shared filter bar narrows every figure to the cards of one project and the
 * cards one agent ran something on (`../scope.ts`), which the server does (`GET /v1/analytics?repository=&agent=`);
 * the filters are in the address. Charts follow the Usage page: category colours, figures in `tabular-nums`, and every
 * chart has its numbers in words or in a table.
 */
import { useQuery } from "@tanstack/react-query";
import { analytics } from "../../api/calls/analytics";
import { sayError } from "../../api/errorText";
import { keys } from "../../api/keys";
import type { Analytics, BurnUp, Spread } from "../../api/types/analytics";
import { Button } from "../../ui/components/Button";
import { FilterBar } from "../../ui/filters/FilterBar";
import { noun, plural } from "../../ui/lib/format";
import { Box } from "../../ui/page/Box";
import { Chip } from "../../ui/page/Chip";
import { Empty } from "../../ui/page/Empty";
import { Figures } from "../../ui/page/Figures";
import { CELL, Table } from "../../ui/page/List";
import { Notice } from "../../ui/page/Notice";
import { PageHeader } from "../../ui/page/PageHeader";
import { Reading, SkeletonLines } from "../../ui/page/Skeleton";
import { chart } from "../cycles/model";
import { InsightsTabs } from "../insights/InsightsTabs";
import { useDays, useScopeFilters } from "../scope";
import { grouped, short } from "../usage/figures";
import {
  bars,
  burnWords,
  columnBars,
  DEFAULT_DAYS,
  dayWords,
  isEmpty,
  lengthOf,
  perPoint,
  WINDOWS,
} from "./model";

/** How a spread of times is told under a tile: the middle and the slowest tenth. */
function spreadNote(spread: Spread | null, what: string): string {
  return spread
    ? `${what} · slowest tenth ${lengthOf(spread.p90_seconds)} · ${plural(spread.cards, "card", "cards")}`
    : `No card ${what}`;
}

/** The cards done on each day of the window, as bars with the busiest day the tallest. */
function ThroughputBox({ data }: { data: Analytics }) {
  const days = bars(data.throughput);
  const total = data.totals.done;
  return (
    <Box title="Done each day">
      {total === 0 ? (
        <p className="m-0 text-base text-secondary">No card was done in this window.</p>
      ) : (
        <div className="flex flex-col gap-3">
          <div
            role="img"
            aria-label={`${plural(total, "card", "cards")} done over ${data.days} days, ${data.totals.points_done} points`}
            className="flex h-32 items-end gap-px"
          >
            {days.map((day) => (
              <span
                key={day.day}
                title={`${dayWords(day.day)}: ${plural(day.done, "card", "cards")}, ${day.points} points`}
                className="flex h-full min-w-px flex-1 items-end"
              >
                <span
                  className="block w-full rounded-t-small bg-cat-1"
                  style={{ height: `${day.height}%` }}
                />
              </span>
            ))}
          </div>
          <ul className="sr-only">
            {days
              .filter((day) => day.done > 0)
              .map((day) => (
                <li key={day.day}>
                  {dayWords(day.day)}: {plural(day.done, "card", "cards")}, {day.points} points
                </li>
              ))}
          </ul>
          <div className="flex justify-between text-sm text-secondary">
            <span>{dayWords(data.throughput[0]?.day ?? data.today)}</span>
            <span>{dayWords(data.today)}</span>
          </div>
        </div>
      )}
    </Box>
  );
}

/** How long cards sit in each column, by the mean for one card. */
function ColumnsBox({ data }: { data: Analytics }) {
  const columns = columnBars(data.columns);
  return (
    <Box title="Time in each column">
      {columns.length === 0 ? (
        <p className="m-0 text-base text-secondary">No card has sat in a column in this window.</p>
      ) : (
        // biome-ignore lint/a11y/noRedundantRoles: preflight sets list-style: none, and WebKit then drops a list with no marker from the accessibility tree entirely
        <ul role="list" className="m-0 flex list-none flex-col gap-3 p-0">
          {columns.map((column) => (
            <li key={column.column} className="flex flex-col gap-2">
              <div className="flex flex-wrap items-baseline justify-between gap-3">
                <span className="break-words text-base text-text">
                  {column.column}
                  <span className="text-sm text-secondary">
                    {` · ${plural(column.cards, "card", "cards")}`}
                  </span>
                </span>
                <span className="text-sm tabular-nums text-secondary">
                  {lengthOf(column.mean_seconds)}
                </span>
              </div>
              <div aria-hidden="true" className="h-2 overflow-hidden rounded-full bg-track">
                <span
                  className="block h-full rounded-full bg-cat-2"
                  style={{ width: `${column.width}%` }}
                />
              </div>
            </li>
          ))}
        </ul>
      )}
    </Box>
  );
}

/** What each project got done and what a point cost it. */
function ProjectsBox({ data }: { data: Analytics }) {
  return (
    <Box title="By project">
      {data.projects.length === 0 ? (
        <p className="m-0 text-base text-secondary">No project has a card in these figures.</p>
      ) : (
        <Table
          figures
          head={["Project", "Cards done", "Points done", "Tokens", "Tokens per point"]}
        >
          {data.projects.map((project) => (
            <tr key={project.id}>
              <td className={CELL}>{project.name}</td>
              <td className={`${CELL} text-right tabular-nums`}>{grouped(project.done)}</td>
              <td className={`${CELL} text-right tabular-nums`}>{grouped(project.points_done)}</td>
              <td className={`${CELL} text-right tabular-nums`}>{short(project.tokens)}</td>
              <td className={`${CELL} text-right tabular-nums`}>
                {perPoint(project.tokens_per_point)}
              </td>
            </tr>
          ))}
        </Table>
      )}
    </Box>
  );
}

/** The points planned and the points done of one cycle or milestone, a line each, day by day. */
function BurnChart({ burn }: { burn: BurnUp }) {
  const width = 600;
  const height = 96;
  const lines = chart(burn.days, width, height);
  const last = burn.days[burn.days.length - 1];
  if (!last) return <p className="m-0 text-sm text-secondary">It has not started.</p>;
  return (
    <svg
      role="img"
      aria-label={`Burn-up over ${lines.days} ${noun(lines.days, "day", "days")}: ${last.done} of ${last.scope} points done`}
      viewBox={`-4 -4 ${width + 8} ${height + 8}`}
      className="h-24 w-full"
      preserveAspectRatio="none"
    >
      <polyline
        points={lines.scope}
        fill="none"
        strokeWidth={2}
        vectorEffect="non-scaling-stroke"
        className="stroke-secondary"
        strokeDasharray="4 3"
      />
      <polyline
        points={lines.done}
        fill="none"
        strokeWidth={2}
        vectorEffect="non-scaling-stroke"
        className="stroke-cat-1"
      />
    </svg>
  );
}

/** The burn-up of the latest cycles or milestones: the points planned (dashed) and done (solid). */
function BurnUps({ title, list, none }: { title: string; list: BurnUp[]; none: string }) {
  return (
    <Box title={title} aside="points planned (dashed) and done">
      {list.length === 0 ? (
        <p className="m-0 text-base text-secondary">{none}</p>
      ) : (
        // biome-ignore lint/a11y/noRedundantRoles: preflight sets list-style: none, and WebKit then drops a list with no marker from the accessibility tree entirely
        <ul role="list" className="m-0 flex list-none flex-col gap-6 p-0">
          {list.map((burn) => (
            <li key={burn.id} className="flex flex-col gap-2">
              <div className="flex flex-wrap items-baseline justify-between gap-2">
                <span className="break-words text-base font-semibold text-text">{burn.name}</span>
                <span className="text-sm text-secondary">
                  {burn.project ? `${burn.project} · ` : ""}
                  {dayWords(burn.starts_on)} – {dayWords(burn.ends_on)}
                </span>
              </div>
              <BurnChart burn={burn} />
              <span className="text-sm tabular-nums text-secondary">{burnWords(burn)}</span>
            </li>
          ))}
        </ul>
      )}
    </Box>
  );
}

export function AnalyticsPage() {
  const [days, setDays] = useDays(
    WINDOWS.map((window) => window.days),
    DEFAULT_DAYS,
  );
  const filters = useScopeFilters();
  const { repository = null, agent = "" } = filters.scope;
  const report = useQuery({
    queryKey: keys.analyticsDays(days, repository, agent),
    queryFn: () => analytics(days, filters.scope),
    // Keeps the figures of the window before while the chosen one is read.
    placeholderData: (previous) => previous,
  });
  const data = report.data;
  return (
    <>
      <PageHeader title="Insights" tabs={<InsightsTabs value="insights" />} />
      <FilterBar
        label="Filter the analytics"
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
          {sayError(report.error, "Could not read the analytics. Try again.")}
        </Notice>
      ) : !data ? (
        <Reading message="Reading analytics" className="flex flex-col gap-4">
          <SkeletonLines lines={2} />
          <SkeletonLines lines={3} />
        </Reading>
      ) : isEmpty(data) ? (
        <Empty
          title="Nothing to count yet"
          body="Figures appear once cards are made and moved. Narrow the filters less if some are set."
        />
      ) : (
        <div className="flex flex-col gap-4" data-window={data.days} aria-busy={report.isFetching}>
          <Figures
            items={[
              {
                label: "Cards done",
                value: grouped(data.totals.done),
                hint: `${plural(data.totals.points_done, "point", "points")} · ${data.totals.open} open`,
              },
              {
                label: "Start to done",
                value: data.cycle_time ? lengthOf(data.cycle_time.median_seconds) : "–",
                hint: spreadNote(data.cycle_time, "started"),
              },
              {
                label: "Made to done",
                value: data.lead_time ? lengthOf(data.lead_time.median_seconds) : "–",
                hint: spreadNote(data.lead_time, "done"),
              },
              {
                label: "Tokens per point",
                value: perPoint(data.totals.tokens_per_point),
                hint: `${short(data.totals.tokens)} tokens on the cards done`,
              },
            ]}
          />
          <ThroughputBox data={data} />
          <ColumnsBox data={data} />
          <ProjectsBox data={data} />
          <BurnUps title="Cycles" list={data.cycles} none="No cycle has a card in these figures." />
          <BurnUps
            title="Milestones"
            list={data.milestones}
            none="No milestone has a card in these figures."
          />
        </div>
      )}
    </>
  );
}
