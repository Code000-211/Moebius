/**
 * The Usage page (the desktop app's `screens/usage`): what the model calls used, from `GET /v1/usage`. A window
 * (today, 7 or 30 days) sets the tiles, the models, what the model did with its time, the agents and the tool waits;
 * the heatmap always shows the last year. The tokens used today also feed the meter at the foot of the sidebar.
 */
import { useQuery } from "@tanstack/react-query";
import { useState } from "react";
import { usage } from "../../api/calls/usage";
import { keys } from "../../api/keys";
import { Loader } from "../../ui/components/Loader";
import { Box } from "../../ui/page/Box";
import { Chip } from "../../ui/page/Chip";
import { Header } from "../shell/Page";
import { AgentsBox, LensBox, ModelsBox, TimeBox, WaitsBox } from "./Boxes";
import { Heatmap } from "./Heatmap";
import { span, tiles, WINDOWS } from "./model";

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
      <Header title="Usage" />
      <p className="m-0 mb-[12px] text-[12.5px] text-normal">
        Which models ran when, where the tokens went, and what the models spent time on.
      </p>
      <fieldset className="m-0 mb-[12px] flex flex-wrap gap-base border-0 p-0">
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
        <p role="alert" className="text-sm text-error">
          {report.error.message}
        </p>
      ) : !data ? (
        <Loader message="Reading the usage…" className="py-double" />
      ) : (
        <div className="flex flex-col gap-[12px]" data-window={data.days_in_window}>
          <div className="grid gap-[12px] [grid-template-columns:repeat(auto-fit,minmax(200px,1fr))]">
            {tiles(data, data.days_in_window).map((tile) => (
              <Box key={tile.label}>
                <div className="text-[11px] font-semibold text-low">{tile.label}</div>
                <div className="text-[26px] font-bold text-high">{tile.value}</div>
                <div className="font-mono text-[11px] text-low">{tile.note}</div>
              </Box>
            ))}
          </div>
          <Heatmap report={data} />
          <ModelsBox report={data} span={span(data.days_in_window)} />
          <div className="grid gap-[12px] lg:grid-cols-2">
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
