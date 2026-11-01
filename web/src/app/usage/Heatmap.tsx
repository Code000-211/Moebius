/** The heatmap of the Usage page: the last year, a cell a day, in the colour of the model that led the day and one of four shades by its tokens. */
import type { UsageReport } from "../../api/types/usage";
import { Box } from "../../ui/page/Box";
import { short, WEEKDAYS } from "./figures";
import { type Cell, heat, modelColours, SHADE_WEIGHT, WEEKS } from "./model";

/** A shade of the colour of model `n` against an empty cell: level 1 (dimmest) to 4 (the colour itself). */
export function shade(n: number, level: number): string {
  const weight = SHADE_WEIGHT[Math.min(4, Math.max(1, level)) - 1];
  return `color-mix(in srgb, var(--model-${n}) ${weight}%, var(--heat-empty))`;
}

export function Heatmap({ report }: { report: UsageReport }) {
  const map = heat(report);
  if (!map) {
    return (
      <Box title="Tokens per day">
        <p className="m-0 text-[13px] text-normal">The date is not known.</p>
      </Box>
    );
  }
  const colour = modelColours(report);
  const background = (cell: Cell): string => {
    if (cell.level === null) return "transparent";
    if (cell.level === 0 || cell.model === null) return "var(--heat-empty)";
    return shade(colour(cell.model), cell.level);
  };
  const columns = `36px repeat(${WEEKS}, minmax(10px, 1fr))`;
  return (
    <Box title="Tokens per day" aside={map.aside}>
      <div className="overflow-x-auto">
        <div className="min-w-[760px]" data-testid="heatmap">
          <div
            className="grid gap-[2px]"
            style={{
              gridTemplateColumns: `36px ${map.months.map((m) => `${m.weeks}fr`).join(" ")}`,
            }}
          >
            <span />
            {map.months.map((month, at) => (
              <span
                // biome-ignore lint/suspicious/noArrayIndexKey: the months are drawn once, in order
                key={at}
                className="overflow-hidden whitespace-nowrap font-mono text-[11px] text-low"
              >
                {month.name}
              </span>
            ))}
          </div>
          {map.rows.map((row, at) => (
            <div
              // biome-ignore lint/suspicious/noArrayIndexKey: the seven rows are the days of the week
              key={at}
              className="mt-[2px] grid items-center gap-[2px]"
              style={{ gridTemplateColumns: columns }}
            >
              <span className="font-mono text-[11px] text-low">
                {at % 2 === 1 ? WEEKDAYS[at] : ""}
              </span>
              {row.map((cell) => (
                <span
                  key={cell.day}
                  title={
                    cell.level === null
                      ? undefined
                      : `${cell.date}: ${short(cell.tokens)} tokens${cell.model ? ` (${cell.model})` : ""}`
                  }
                  className="aspect-square rounded-[3px]"
                  style={{ background: background(cell) }}
                />
              ))}
            </div>
          ))}
        </div>
      </div>
      <ul className="m-0 mt-base flex list-none flex-wrap items-center gap-[16px] p-0">
        {map.leading.map((model) => (
          <li key={model} className="flex items-center gap-base">
            <span className="flex gap-[2px]">
              {[1, 2, 3, 4].map((level) => (
                <span
                  key={level}
                  className="size-[12px] rounded-[3px]"
                  style={{ background: shade(colour(model), level) }}
                />
              ))}
            </span>
            <span className="font-mono text-[11px] text-normal">{model}</span>
          </li>
        ))}
        <li className="font-mono text-[11px] text-low">Lighter means fewer tokens, darker more</li>
      </ul>
    </Box>
  );
}
