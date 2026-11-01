/**
 * The Plugins page: what the plugin host loaded (`GET /v1/plugins`), which answer the decisions. It only shows: each
 * plugin's version, kinds, whether it runs in the server or as its own process and in what state, and the kinds of
 * decision it answers; and for plugins in their own process the program, how often it was started again, why it last
 * stopped and where its log is.
 */
import { useQuery } from "@tanstack/react-query";
import { plugins as readPlugins } from "../../api/calls/models";
import { keys } from "../../api/keys";
import type { PluginProcess } from "../../api/types/models";
import { Loader } from "../../ui/components/Loader";
import { cn } from "../../ui/lib/cn";
import { Box, Notice } from "../harness/form";
import { Header } from "../shell/Page";

/** How a plugin runs: in the server, or as its own process in a state, coloured by whether it takes calls. */
export function runsLabel(process: PluginProcess | null): { label: string; tone: string } {
  if (process === null) return { label: "In the server", tone: "text-normal" };
  switch (process.state) {
    case "running":
      return { label: "Running", tone: "text-success" };
    case "restarting":
      return { label: "Restarting", tone: "text-amber" };
    case "stopped":
      return { label: "Stopped", tone: "text-error" };
    default:
      return { label: "Starting", tone: "text-low" };
  }
}

/** How often a plugin in its own process was started again. */
export function restartsLabel(restarts: number): string {
  return restarts === 0 ? "Never" : restarts === 1 ? "Once" : `${restarts} times`;
}

function Table({ heads, rows }: { heads: string[]; rows: React.ReactNode[][] }) {
  return (
    <div className="overflow-x-auto">
      <table className="w-full text-left text-sm">
        <thead>
          <tr className="border-b border-line text-xs text-low">
            {heads.map((head) => (
              <th key={head} className="py-1 pr-base font-semibold">
                {head}
              </th>
            ))}
          </tr>
        </thead>
        <tbody>
          {rows.map((cells, row) => (
            // biome-ignore lint/suspicious/noArrayIndexKey: the rows are in the order the server listed them
            <tr key={row} data-testid="plugin-row" className="border-b border-line/50 align-top">
              {cells.map((cell, column) => (
                // biome-ignore lint/suspicious/noArrayIndexKey: a cell is its column
                <td key={column} className="py-1.5 pr-base text-normal">
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

const mono = (text: string) => (
  <span className="font-mono text-xs text-high [overflow-wrap:anywhere]">{text}</span>
);

export function PluginsPage() {
  const plugins = useQuery({ queryKey: keys.plugins, queryFn: readPlugins });
  if (plugins.isPending) {
    return (
      <>
        <Header breadcrumb="Setup" title="Plugins" />
        <Loader message="Reading the plugins…" className="py-double" />
      </>
    );
  }
  if (plugins.isError) {
    return (
      <>
        <Header breadcrumb="Setup" title="Plugins" />
        <Notice tone="error">{plugins.error.message}</Notice>
      </>
    );
  }
  const { plugins: loaded, time_limit_seconds: limit } = plugins.data;
  const processes = loaded.flatMap((plugin) =>
    plugin.process ? [{ plugin, process: plugin.process }] : [],
  );
  return (
    <>
      <Header breadcrumb="Setup" title="Plugins" />
      <p className="m-0 mb-plusfifty text-sm text-normal">
        {loaded.length} plugins loaded; a call into one is refused after {limit} s.
      </p>
      <div className="flex flex-col gap-double">
        <Box title="Loaded plugins">
          {loaded.length === 0 ? (
            <p className="m-0 text-sm text-normal">No plugin is loaded.</p>
          ) : (
            <Table
              heads={["Plugin", "Version", "Kinds", "Runs", "Decides"]}
              rows={loaded.map((plugin) => {
                const runs = runsLabel(plugin.process);
                return [
                  mono(plugin.name),
                  plugin.version,
                  plugin.kinds.join(", "),
                  <span key="runs" className={cn("font-semibold", runs.tone)}>
                    {runs.label}
                  </span>,
                  plugin.decides.length === 0
                    ? "No kind of decision"
                    : mono(plugin.decides.join(", ")),
                ];
              })}
            />
          )}
        </Box>
        {processes.length > 0 && (
          <Box title="Plugins in their own process">
            <Table
              heads={["Plugin", "Program", "Restarted", "Why it last stopped", "Log"]}
              rows={processes.map(({ plugin, process }) => [
                mono(plugin.name),
                mono(process.program),
                restartsLabel(process.restarts),
                process.error ?? "It never stopped",
                mono(process.log),
              ])}
            />
          </Box>
        )}
      </div>
    </>
  );
}
