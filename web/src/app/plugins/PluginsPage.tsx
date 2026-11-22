/**
 * The Plugins page: what the plugin host loaded (`GET /v1/plugins`), which answer the decisions: each
 * plugin's version, kinds, whether it runs in the server or as its own process and in what state, and the kinds of
 * decision it answers; and for plugins in their own process the program, how often it was started again, why it last
 * stopped and where its log is. Each plugin named in moebius.toml has a switch that turns it off and on at once; one
 * that is off shows as "Off", greyed, and can be turned on again. The plugins are named in moebius.toml, or installed
 * from the registries: the page lists the installed packages and opens the package sheet on them ("Browse
 * packages"). What each plugin asks of the whole server is a section of the page, one box per plugin; what it asks
 * of one repository is on that repository's page.
 */
import { useQuery } from "@tanstack/react-query";
import type { ReactNode } from "react";
import { appSettings, plugins as readPlugins, saveAppSettings } from "../../api/calls/models";
import { keys } from "../../api/keys";
import type { PluginProcess } from "../../api/types/models";
import { usePackageSheet } from "../../packages/PackageSheetContext";
import { Button } from "../../ui/components/Button";
import { plural } from "../../ui/lib/format";
import { Empty } from "../../ui/page/Empty";
import { Notice } from "../../ui/page/Notice";
import { SkeletonCard } from "../../ui/page/Skeleton";
import type { Tone } from "../../ui/page/StateLabel";
import { StateLabel } from "../../ui/page/StateLabel";
import { AreaTitle, Section } from "../settings/parts";
import { InstalledPackages } from "./InstalledPackages";
import { PluginSettingsBoxes, type SettingsScope } from "./PluginSettingsBoxes";
import { PluginSwitch, SWITCHED_BUILT_INS } from "./PluginSwitch";

/** What running plugins ask of the whole server (`app_settings` of their manifests): one box per plugin. */
const APP_SCOPE: SettingsScope = {
  kind: "app",
  repository: null,
  queryKey: keys.appSettings,
  read: appSettings,
  save: saveAppSettings,
  empty:
    "No plugin on this server asks anything of it. One that does, such as a sandbox with a base image for every repository, shows its settings here.",
};

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

/** A plugin: its name with the state it is in and its switch on one row, its facts under it as rows. */
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
    <div data-testid="plugin-row" data-off={off || undefined} className="border-t border-hairline">
      <div className="flex min-h-9 flex-wrap items-center justify-between gap-x-6 gap-y-1 pt-2">
        <h3
          className={`m-0 min-w-0 break-all font-mono text-sm font-semibold ${off ? "text-secondary" : "text-text"}`}
        >
          {name}
        </h3>
        <div className="flex flex-wrap items-center gap-4">
          <StateLabel tone={state.tone}>{state.label}</StateLabel>
          {control}
        </div>
      </div>
      <dl className={`m-0 flex flex-col pb-2 ${off ? "opacity-60" : ""}`}>{facts}</dl>
    </div>
  );
}

/** One fact of a plugin: its name at the left, its value at the right, monospace only where a character matters. */
function Fact({ label, value, mono }: { label: string; value: ReactNode; mono?: boolean }) {
  return (
    <div className="flex min-h-7 flex-wrap items-baseline justify-between gap-x-6">
      <dt className="text-sm text-muted">{label}</dt>
      <dd
        className={`m-0 min-w-0 break-words text-right text-sm text-text tabular-nums ${mono ? "font-mono" : ""}`}
      >
        {value}
      </dd>
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
        <AreaTitle title="Plugins" actions={browse} />
        <SkeletonCard lines={4} message="Reading the plugins" />
      </>
    );
  }
  if (plugins.isError) {
    return (
      <>
        <AreaTitle title="Plugins" actions={browse} />
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
      <AreaTitle title="Plugins" actions={browse} />
      <div className="flex flex-col">
        <InstalledPackages onBrowse={() => sheet.open("moebius")} />
        <Section title="Settings of plugins" aside="What a plugin asks of the whole server.">
          <PluginSettingsBoxes scope={APP_SCOPE} />
        </Section>
        <Section
          title="Loaded plugins"
          aside={`${plural(loaded.length, "plugin", "plugins")}; a call into one is refused after ${limit} s.`}
        >
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
                  // A plugin named under [[plugins]] has a process or is off; of the ones built in, the guard and
                  // the localizer have a switch.
                  plugin.process !== null ||
                  !plugin.enabled ||
                  SWITCHED_BUILT_INS.has(plugin.name) ? (
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
        </Section>
        {processes.length > 0 && (
          <Section
            title="Plugins in their own process"
            aside="Started with the server; a refusal or a stop is written to its log."
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
          </Section>
        )}
      </div>
    </>
  );
}
