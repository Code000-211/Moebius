/**
 * The Plugins page: what the plugin host loaded (`GET /v1/plugins`), which answer the decisions: each
 * plugin's version, kinds, whether it runs in the server or as its own process and in what state, and the kinds of
 * decision it answers; and for plugins in their own process the program, how often it was started again, why it last
 * stopped and where its log is. Each plugin named in moebius.toml has a switch that turns it off and on at once; one
 * that is off shows as "Off", greyed, and can be turned on again. The plugins are named in moebius.toml, or installed
 * from the registries: the page lists the installed packages and opens the package sheet on them ("Browse
 * packages").
 */
import { useQuery } from "@tanstack/react-query";
import type { ReactNode } from "react";
import { plugins as readPlugins } from "../../api/calls/models";
import { keys } from "../../api/keys";
import type { PluginProcess } from "../../api/types/models";
import { usePackageSheet } from "../../packages/PackageSheetContext";
import { Button } from "../../ui/components/Button";
import { plural } from "../../ui/lib/format";
import { Box } from "../../ui/page/Box";
import { Empty } from "../../ui/page/Empty";
import { Notice } from "../../ui/page/Notice";
import { PageHeader } from "../../ui/page/PageHeader";
import { SkeletonCard } from "../../ui/page/Skeleton";
import { Status, type Tone } from "../../ui/page/Status";
import { InstalledPackages } from "./InstalledPackages";
import { PluginSwitch } from "./PluginSwitch";

/**
 * How a plugin runs: built into the server, or as its own process in a state, coloured by whether it takes calls.
 * A plugin that is turned off says "Off", whatever it was. "Built in" is the words the app already uses for what Moebius brings (an agent, a skill, a script), so a person who
 * has read it once reads it the same way here.
 */
export function runsLabel(
  process: PluginProcess | null,
  enabled = true,
): { label: string; tone: Tone } {
  if (!enabled) return { label: "Off", tone: "neutral" };
  if (process === null) return { label: "Built in", tone: "neutral" };
  switch (process.state) {
    case "running":
      return { label: "Running", tone: "complete" };
    case "restarting":
      return { label: "Restarting", tone: "attention" };
    case "stopped":
      return { label: "Stopped", tone: "error" };
    default:
      return { label: "Starting", tone: "progress" };
  }
}

/** How often a plugin in its own process was started again. */
export function restartsLabel(restarts: number): string {
  return restarts === 0 ? "Never" : restarts === 1 ? "Once" : `${restarts} times`;
}

/** One fact of a plugin: its name above, its value under, monospace only where a character matters. */
function Fact({ label, value, mono }: { label: string; value: ReactNode; mono?: boolean }) {
  return (
    <div className="flex min-w-0 flex-col gap-1">
      <span className="text-sm font-semibold text-secondary">{label}</span>
      <span
        className={`min-w-0 break-words text-base text-text tabular-nums ${mono ? "font-mono text-sm" : ""}`}
      >
        {value}
      </span>
    </div>
  );
}

/** A plugin, with the state it is in at its right and its facts under. */
function PluginRow({
  name,
  state,
  facts,
  off = false,
  control,
}: {
  name: string;
  state: { label: string; tone: Tone };
  facts: ReactNode;
  /** Whether the plugin is turned off: its name and facts are greyed, the switch is not. */
  off?: boolean;
  /** The switch that turns the plugin off or on. */
  control?: ReactNode;
}) {
  return (
    <div data-testid="plugin-row" data-off={off || undefined} className="flex flex-col gap-4">
      <div className="flex flex-wrap items-center justify-between gap-4">
        <h3
          className={`m-0 min-w-0 break-all font-mono text-sm font-semibold ${off ? "text-secondary" : "text-text"}`}
        >
          {name}
        </h3>
        <div className="flex flex-wrap items-center gap-4">
          <Status tone={state.tone}>{state.label}</Status>
          {control}
        </div>
      </div>
      <div className={`grid gap-4 sm:grid-cols-3 ${off ? "opacity-60" : ""}`}>{facts}</div>
    </div>
  );
}

export function PluginsPage() {
  const sheet = usePackageSheet();
  const browse = (
    <Button variant="secondary" size="sm" onClick={() => sheet.open("moebius")}>
      Browse packages
    </Button>
  );
  const plugins = useQuery({ queryKey: keys.plugins, queryFn: readPlugins });
  if (plugins.isPending) {
    return (
      <>
        <PageHeader title="Plugins" actions={browse} />
        <SkeletonCard lines={4} message="Reading the plugins" />
      </>
    );
  }
  if (plugins.isError) {
    return (
      <>
        <PageHeader title="Plugins" actions={browse} />
        <Notice
          tone="error"
          action={
            <Button
              size="sm"
              variant="secondary"
              disabled={plugins.isFetching}
              onClick={() => void plugins.refetch()}
            >
              Try again
            </Button>
          }
        >
          {`Could not read what the plugin host loaded. ${plugins.error.message}`}
        </Notice>
      </>
    );
  }
  const { plugins: loaded, time_limit_seconds: limit } = plugins.data;
  const processes = loaded.flatMap((plugin) =>
    plugin.process ? [{ plugin, process: plugin.process }] : [],
  );
  return (
    <>
      <PageHeader
        title="Plugins"
        actions={browse}
        context={`${plural(loaded.length, "plugin", "plugins")} loaded; a call into one is refused after ${limit} s.`}
      />
      <div className="flex flex-col gap-8">
        <InstalledPackages onBrowse={() => sheet.open("moebius")} />
        <Box title="Loaded plugins" className="flex flex-col gap-6">
          {loaded.length === 0 ? (
            <Empty
              plain
              title="No plugin loaded"
              body="A plugin answers a decision Moebius asks about, such as whether a tool call may run. Name one under [[plugins]] in moebius.toml and restart the server."
            />
          ) : (
            loaded.map((plugin) => (
              <PluginRow
                key={plugin.name}
                name={plugin.name}
                state={runsLabel(plugin.process, plugin.enabled)}
                off={!plugin.enabled}
                control={
                  // A plugin named under [[plugins]] has a process or is off; the ones built in are always on.
                  plugin.process !== null || !plugin.enabled ? (
                    <PluginSwitch name={plugin.name} enabled={plugin.enabled} />
                  ) : undefined
                }
                facts={
                  <>
                    <Fact label="Version" value={plugin.version || "Not given"} />
                    <Fact
                      label="Kinds"
                      mono
                      value={plugin.kinds.length === 0 ? "None stated" : plugin.kinds.join(", ")}
                    />
                    <Fact
                      label="Answers"
                      mono
                      value={
                        plugin.decides.length === 0
                          ? "No kind of decision"
                          : plugin.decides.join(", ")
                      }
                    />
                  </>
                }
              />
            ))
          )}
        </Box>
        {processes.length > 0 && (
          <Box
            title="Plugins in their own process"
            aside="Started with the server; a refusal or a stop is written to its log."
            className="flex flex-col gap-6"
          >
            {processes.map(({ plugin, process }) => {
              const runs = runsLabel(process);
              return (
                <PluginRow
                  key={plugin.name}
                  name={plugin.name}
                  state={runs}
                  facts={
                    <>
                      <Fact label="Program" mono value={process.program} />
                      <Fact label="Restarted" value={restartsLabel(process.restarts)} />
                      <Fact
                        label="Why it last stopped"
                        value={process.error ?? "It never stopped"}
                      />
                      <Fact label="Log" mono value={process.log} />
                    </>
                  }
                />
              );
            })}
          </Box>
        )}
      </div>
    </>
  );
}
