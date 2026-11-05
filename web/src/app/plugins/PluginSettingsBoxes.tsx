/**
 * The settings plugins ask of a person, one box per plugin that declares some in its manifest: those of a repository
 * on its page (`project_settings`) and those of the whole server on the Settings page (`app_settings`). The boxes know
 * nothing of any plugin: each setting is drawn from what the plugin declares, a field of one line or a code editor
 * (loaded when a plugin asks for one) in the language it names. Save replaces all of one plugin's values in the scope;
 * the plugin uses them from its next call, a launcher from the next run.
 */
import { type QueryKey, useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { lazy, Suspense, useState } from "react";
import { isApiError } from "../../api/http";
import type { PluginSettings as Plugin, PluginSetting } from "../../api/types/projects";
import { Button } from "../../ui/components/Button";
import { Input } from "../../ui/components/Input";
import { Box } from "../../ui/page/Box";
import { Field } from "../../ui/page/Field";
import { Notice } from "../../ui/page/Notice";
import { ReadFailed } from "../../ui/page/ReadFailed";
import { SkeletonLines } from "../../ui/page/Skeleton";

const CodeEditor = lazy(() => import("../../ui/CodeEditor"));

/** How the boxes of one scope read and save: the query's key, the read, the save of one plugin's values. */
export interface SettingsScope {
  queryKey: QueryKey;
  read: () => Promise<{ plugins: Plugin[] }>;
  save: (plugin: string, values: Record<string, string>) => Promise<Plugin>;
  /** What the page says when no plugin asks anything in this scope. */
  empty: string;
}

export function PluginSettingsBoxes({ scope }: { scope: SettingsScope }) {
  const read = useQuery({ queryKey: scope.queryKey, queryFn: scope.read });
  if (read.isPending) {
    return (
      <Box title="Plugin settings">
        <SkeletonLines lines={3} message="Reading the plugins' settings" />
      </Box>
    );
  }
  if (read.isError) {
    return (
      <Box title="Plugin settings">
        <ReadFailed
          error={read.error}
          fallback="The plugins' settings could not be read. Try again."
          busy={read.isFetching}
          onRetry={() => void read.refetch()}
        />
      </Box>
    );
  }
  if (read.data.plugins.length === 0) {
    return (
      <Box title="Plugin settings">
        <p className="m-0 max-w-prose text-base text-secondary">{scope.empty}</p>
      </Box>
    );
  }
  return (
    <>
      {read.data.plugins.map((plugin) => (
        <PluginBox key={plugin.plugin} scope={scope} plugin={plugin} />
      ))}
    </>
  );
}

type Saved = { tone: "ok" | "error"; text: string };

/** One plugin's box. What came of the last save lives here: the form starts again when the saved values change. */
function PluginBox({ scope, plugin }: { scope: SettingsScope; plugin: Plugin }) {
  const [notice, setNotice] = useState<Saved | null>(null);
  return (
    <Box
      title={plugin.plugin}
      aside={`Plugin settings · ${plugin.version}`}
      testId="plugin-settings"
    >
      <PluginForm
        key={plugin.settings.map((setting) => setting.value).join("\u0000")}
        scope={scope}
        plugin={plugin}
        notice={notice}
        setNotice={setNotice}
      />
    </Box>
  );
}

function PluginForm({
  scope,
  plugin,
  notice,
  setNotice,
}: {
  scope: SettingsScope;
  plugin: Plugin;
  notice: Saved | null;
  setNotice: (notice: Saved | null) => void;
}) {
  const queryClient = useQueryClient();
  const saved = Object.fromEntries(plugin.settings.map((setting) => [setting.key, setting.value]));
  const [values, setValues] = useState<Record<string, string>>(saved);
  const changed = plugin.settings.some((setting) => values[setting.key] !== setting.value);
  const save = useMutation({
    mutationFn: () => scope.save(plugin.plugin, values),
    onSuccess: (answer) => {
      setNotice({ tone: "ok", text: "Saved. The plugin uses it from the next run." });
      queryClient.setQueryData<{ plugins: Plugin[] }>(scope.queryKey, (old) =>
        old
          ? {
              ...old,
              plugins: old.plugins.map((each) => (each.plugin === answer.plugin ? answer : each)),
            }
          : old,
      );
    },
    onError: (error) =>
      setNotice({
        tone: "error",
        text: isApiError(error) ? error.message : "The settings could not be saved.",
      }),
  });
  const edit = (key: string, value: string) => {
    setValues((old) => ({ ...old, [key]: value }));
    setNotice(null);
  };
  return (
    <form
      className="flex flex-col gap-4"
      onSubmit={(e) => {
        e.preventDefault();
        if (changed && !save.isPending) save.mutate();
      }}
    >
      {plugin.settings.map((setting) => (
        <SettingControl
          key={setting.key}
          setting={setting}
          value={values[setting.key] ?? ""}
          onChange={(value) => edit(setting.key, value)}
        />
      ))}
      {notice && (
        <Notice tone={notice.tone}>
          {notice.tone === "error" && (
            <span className="font-semibold">Couldn't save. The settings stay as they were. </span>
          )}
          {notice.text}
        </Notice>
      )}
      <div className="flex flex-wrap gap-2">
        <Button type="submit" size="sm" disabled={!changed || save.isPending}>
          {save.isPending ? "Saving" : "Save settings"}
        </Button>
        <Button
          type="button"
          size="sm"
          variant="secondary"
          disabled={!changed}
          onClick={() => {
            setValues(saved);
            setNotice(null);
          }}
        >
          Revert changes
        </Button>
      </div>
    </form>
  );
}

/** One setting, drawn as the plugin declares it. */
function SettingControl({
  setting,
  value,
  onChange,
}: {
  setting: PluginSetting;
  value: string;
  onChange: (value: string) => void;
}) {
  if (setting.input === "line") {
    return (
      <Field label={setting.label} hint={setting.description || undefined} className="max-w-xl">
        <Input value={value} autoComplete="off" onChange={(e) => onChange(e.target.value)} />
      </Field>
    );
  }
  // The editor is not a control a <label> can name, so it carries its own name and the label is only shown.
  return (
    <div className="flex min-w-0 flex-col gap-2">
      <span className="text-sm font-semibold text-secondary">{setting.label}</span>
      {setting.description && (
        <span className="max-w-prose text-sm text-secondary">{setting.description}</span>
      )}
      <Suspense fallback={<SkeletonLines lines={6} message="Opening the editor" />}>
        <CodeEditor
          label={setting.label}
          language={setting.language ?? ""}
          value={value}
          onChange={onChange}
        />
      </Suspense>
    </div>
  );
}
