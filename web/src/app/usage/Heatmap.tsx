/**
 * The heatmap of the Usage page: the last year, a cell a day, coloured by the model that led the day and shaded by the
 * day's tokens against the busiest day. Colour carries the picture but never the meaning: every cell says its date,
 * its tokens and its model as text, the ground colour is named in the legend, and the box sums the year up in words.
 */
import type { UsageReport } from "../../api/types/usage";
import { cn } from "../../ui/lib/cn";
import { dayName } from "../../ui/lib/format";
import { Box } from "../../ui/page/Box";
import { short, WEEKDAYS } from "./figures";
import { type Cell, heat, modelColours, SHADE_WEIGHT, WEEKS } from "./model";

/**
 * A shade of category colour `n` against an empty cell: level 1 (the faintest) to 4 (the colour itself). Both halves
 * of the mix are tokens of `styles/theme.css` — `--cat-<n>` for the model and `--heat-empty` for the ground of an
 * empty cell — each theme with values of its own, so the ramp grows away from the ground in either: towards the
 * strongest colour and away from the pale ground in light, towards the strongest colour and away from the dark ground
 * in dark. No colour is written out here.
 */
export function shade(n: number, level: number): string {
  const weight = SHADE_WEIGHT[Math.min(4, Math.max(1, level)) - 1];
  return `color-mix(in srgb, var(--cat-${n}) ${weight}%, var(--heat-empty))`;
}

/**
 * What a cell is: its day, its tokens and its model, so the colour is never the only sign. The day is named from what
 * the server says today is, so a heatmap read at midnight still calls the right cell Today.
 */
function describe(cell: Cell, today: string): string | undefined {
  if (cell.level === null) return undefined;
  const day = dayName(cell.date, today);
  if (cell.level === 0) return `${day}: no tokens`;
  return `${day}: ${short(cell.tokens)} tokens${cell.model ? ` (${cell.model})` : ""}`;
}

export function Heatmap({ report }: { report: UsageReport }) {
  const map = heat(report);
  if (!map) {
    return (
      <Box title="Tokens per day">
        <p className="m-0 text-base text-secondary">
          The server did not say what day it is, so there is no year to draw.
        </p>
      </Box>
    );
  }
  const colour = modelColours(report);
  const background = (cell: Cell): string | undefined => {
    if (cell.level === null || cell.level === 0 || cell.model === null) return undefined;
    return shade(colour(cell.model), cell.level);
  };
  const columns = `2.5rem repeat(${WEEKS}, minmax(10px, 1fr))`;
  return (
    <Box title="Tokens per day" aside={map.aside}>
      <div className="overflow-x-auto">
        <div data-testid="heatmap">
          <div
            className="grid gap-px"
            style={{
              gridTemplateColumns: `2.5rem ${map.months.map((m) => `${m.weeks}fr`).join(" ")}`,
            }}
          >
            <span />
            {map.months.map((month, at) => (
              <span
                // biome-ignore lint/suspicious/noArrayIndexKey: the months are drawn once, in order
                key={at}
                className="overflow-hidden whitespace-nowrap text-xs text-secondary"
              >
                {month.name}
              </span>
            ))}
          </div>
          {map.rows.map((row, at) => (
            <div
              // biome-ignore lint/suspicious/noArrayIndexKey: the seven rows are the days of the week
              key={at}
              className="mt-px grid items-center gap-px"
              style={{ gridTemplateColumns: columns }}
            >
              <span className="text-xs text-secondary">{at % 2 === 1 ? WEEKDAYS[at] : ""}</span>
              {row.map((cell) => (
                <span
                  key={cell.day}
                  title={describe(cell, report.today)}
                  className={cn(
                    "aspect-square rounded-small",
                    cell.level === null && "bg-transparent",
                    cell.level === 0 && "bg-heat",
                  )}
                  style={
                    background(cell) === undefined ? undefined : { background: background(cell) }
                  }
                />
              ))}
            </div>
          ))}
        </div>
      </div>
      {/* biome-ignore lint/a11y/noRedundantRoles: preflight sets list-style: none, and WebKit then drops a list with no marker from the accessibility tree entirely */}
      <ul role="list" className="m-0 mt-3 flex list-none flex-wrap items-center gap-4 p-0">
        {map.leading.map((model) => (
          <li key={model} className="flex items-center gap-2">
            <span aria-hidden="true" className="flex gap-px">
              {[1, 2, 3, 4].map((level) => (
                <span
                  key={level}
                  className="size-3 rounded-small"
                  style={{ background: shade(colour(model), level) }}
                />
              ))}
            </span>
            <span className="break-words font-mono text-sm text-secondary">{model}</span>
          </li>
        ))}
        <li className="flex items-center gap-2">
          <span aria-hidden="true" className="size-3 rounded-small bg-heat" />
          <span className="text-sm text-secondary">No tokens that day</span>
        </li>
        <li className="text-sm text-secondary">
          A model keeps one colour. The stronger the colour, the more tokens that day used.
        </li>
      </ul>
    </Box>
  );
}
