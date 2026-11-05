/**
 * The Settings page (the desktop app's Settings in `screens/setup`): every key of `moebius.toml` as an input in a box,
 * as `GET /v1/settings` tells them, whose Save sends only the keys that were changed through `PUT /v1/settings`. The
 * server checks the file as `moebius serve` would read it and answers 422 with the message when it does not hold
 * together; that message is shown as it came, and a key whose own input cannot be typed right says so under the input
 * before anything is sent. A key the running server uses only after a restart makes a banner with a Restart button.
 * The page also shows the webhooks of the forges and where this server runs. What plugins ask of the server is on the
 * Plugin settings page.
 */
import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { useState } from "react";
import { serverInfo } from "../../api/calls/server";
import { saveSettings, settings } from "../../api/calls/settings";
import { sayError } from "../../api/errorText";
import { keys } from "../../api/keys";
import type { ServerSettings } from "../../api/types/settings";
import { Button } from "../../ui/components/Button";
import { Input } from "../../ui/components/Input";
import { Box } from "../../ui/page/Box";
import { Field, Select } from "../../ui/page/Field";
import { Notice } from "../../ui/page/Notice";
import { Body, PageHeader } from "../../ui/page/PageHeader";
import { Reading, SkeletonCard } from "../../ui/page/Skeleton";
import { Appearance } from "./Appearance";
import type { Form } from "./fields";
import {
  changed,
  changes,
  fieldValue,
  nothingToSave,
  readForms,
  type Field as Setting,
} from "./fields";
import { Code, KeyValue, KeyValueGrid } from "./parts";
import { RestartBanner } from "./RestartBanner";
import { WebhooksBox } from "./Webhooks";

type Outcome = { tone: "ok" | "error"; text: string } | null;

export function SettingsPage() {
  const data = useQuery({ queryKey: keys.settings, queryFn: () => settings() });
  const header = <PageHeader title="Settings" />;
  // The heading stays while the file is read, so the page does not move under a person who is already looking at it.
  if (data.isPending) {
    return (
      <>
        {header}
        <Reading message="Reading the settings" className="grid items-start gap-6 xl:grid-cols-2">
          <SkeletonCard lines={4} />
          <SkeletonCard lines={4} />
        </Reading>
      </>
    );
  }
  if (data.isError || !data.data) {
    return (
      <>
        {header}
        <Notice tone="error">
          {sayError(data.error, "Could not read the settings. Try again.")}
        </Notice>
      </>
    );
  }
  return <SettingsForm initial={data.data} />;
}

function SettingsForm({ initial }: { initial: ServerSettings }) {
  const queryClient = useQueryClient();
  const [forms, setForms] = useState<Form[]>(() => readForms(initial));
  const [outcome, setOutcome] = useState<Outcome>(null);
  const info = useQuery({ queryKey: keys.server, queryFn: () => serverInfo() });
  const save = useMutation({
    mutationFn: async () => {
      const change = changes(forms);
      if ("error" in change) return { problem: change.error };
      if (nothingToSave(change)) return { nothing: true };
      await saveSettings({ set: change.set, remove: change.remove });
      // The forms are made again from what the file says now.
      const fresh = await settings();
      queryClient.setQueryData(keys.settings, fresh);
      setForms(readForms(fresh));
      return { saved: true };
    },
    onSuccess: (result) => {
      if ("problem" in result) setOutcome({ tone: "error", text: result.problem as string });
      else if ("nothing" in result) setOutcome({ tone: "ok", text: "No changes to save." });
      else setOutcome({ tone: "ok", text: "Saved." });
    },
    onError: (error) =>
      setOutcome({
        tone: "error",
        text: sayError(error, "Could not save the settings. Try again."),
      }),
  });
  const edit = (key: string, text: string) =>
    setForms((current) =>
      current.map((form) => ({
        ...form,
        fields: form.fields.map((each) => (each.key === key ? { ...each, text } : each)),
      })),
    );
  // Save is quiet until a key is different from what the file holds; the reason is the disabled button itself.
  const dirty = forms.some((form) => form.fields.some(changed));

  return (
    <div>
      <PageHeader
        title="Settings"
        actions={
          <Button
            size="sm"
            disabled={save.isPending || !dirty}
            onClick={() => {
              setOutcome(null);
              save.mutate();
            }}
          >
            {save.isPending ? "Saving" : "Save"}
          </Button>
        }
      />
      <div className="flex flex-col gap-6">
        <RestartBanner />
        {outcome && <Notice tone={outcome.tone}>{outcome.text}</Notice>}
        <div className="grid items-start gap-6 xl:grid-cols-2">
          {forms.map((form) => (
            <Box key={form.title} title={form.title} className="min-w-0">
              <div className="flex flex-col gap-4">
                {form.fields.map((each) => (
                  <KeyInput key={each.key} field={each} onChange={(text) => edit(each.key, text)} />
                ))}
              </div>
            </Box>
          ))}
        </div>
        <WebhooksBox />
        <div className="grid items-start gap-6 xl:grid-cols-2">
          <Appearance />
          <ThisServer
            info={info.data}
            pending={info.isPending}
            error={
              info.error ? sayError(info.error, "Could not read where the server runs.") : null
            }
          />
        </div>
      </div>
    </div>
  );
}

/** One key of the file: its input, the default the server uses when the file does not set it, and what is wrong with
 * what was typed while it was being typed. */
function KeyInput({ field, onChange }: { field: Setting; onChange: (text: string) => void }) {
  const showsDefault = field.default !== null && field.text.trim() === "";
  const typed = fieldValue(field);
  const problem = "error" in typed ? typed.error : undefined;
  return (
    <Field
      label={field.label}
      error={problem}
      hint={showsDefault ? `Default ${field.default}` : undefined}
    >
      {field.kind === "yesno" ? (
        <Select
          options={["true", "false"]}
          value={field.text}
          onChange={(picked) => onChange(picked)}
          className="tabular-nums"
        />
      ) : (
        <Input
          value={field.text}
          placeholder={field.default ?? field.whenEmpty ?? ""}
          inputMode={field.kind === "whole" || field.kind === "decimal" ? "numeric" : undefined}
          onChange={(e) => onChange(e.target.value)}
          className={
            field.kind === "code"
              ? "font-mono text-sm"
              : field.kind === "whole" || field.kind === "decimal"
                ? "tabular-nums"
                : undefined
          }
        />
      )}
    </Field>
  );
}

/** What the server says about itself, as the label and the value the page shows. */
function rows(info: NonNullable<Awaited<ReturnType<typeof serverInfo>>>): Array<[string, string]> {
  return [
    ["Program", info.program],
    ["Version", info.version],
    ["Address", info.api_address],
    ["Data directory", info.data_dir],
    ["Host", `${info.host} (${info.os}, ${info.arch})`],
  ];
}

/** Where this server runs, as the server says it. */
function ThisServer({
  info,
  pending,
  error,
}: {
  info: Awaited<ReturnType<typeof serverInfo>> | undefined;
  pending: boolean;
  error: string | null;
}) {
  if (error) {
    return (
      <Box title="This server">
        <Notice tone="error">{error}</Notice>
      </Box>
    );
  }
  return (
    <Box title="This server" aside={pending ? "Reading" : undefined}>
      {info ? (
        <KeyValueGrid>
          {rows(info).map(([label, value]) => (
            <KeyValue key={label} label={label} value={<Code>{value}</Code>} />
          ))}
        </KeyValueGrid>
      ) : (
        <Body>Reading where the server runs</Body>
      )}
    </Box>
  );
}
