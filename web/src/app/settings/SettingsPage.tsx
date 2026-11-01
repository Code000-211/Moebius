/**
 * The Settings page (the desktop app's Settings in `screens/setup`): every key of `moebius.toml` as an input in a box,
 * as `GET /v1/settings` tells them, whose Save sends only the keys that were changed through `PUT /v1/settings`. The
 * server checks the file as `moebius serve` would read it and answers 422 with the message when it does not hold
 * together; that message is shown as it came. A key the running server uses only after a restart makes a banner with a
 * Restart button. The page also shows the webhooks of the forges and where this server runs.
 */
import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { useState } from "react";
import { serverInfo } from "../../api/calls/server";
import { saveSettings, settings } from "../../api/calls/settings";
import { isApiError } from "../../api/http";
import { keys } from "../../api/keys";
import type { ServerSettings } from "../../api/types/settings";
import { Button } from "../../ui/components/Button";
import { Input } from "../../ui/components/Input";
import { Loader } from "../../ui/components/Loader";
import { Box } from "../../ui/page/Box";
import { CONTROL, Field as Labelled } from "../../ui/page/Field";
import { Notice } from "../../ui/page/Notice";
import { Header } from "../shell/Page";
import { changes, type Field, type Form, nothingToSave, readForms } from "./fields";
import { RestartBanner } from "./RestartBanner";
import { WebhooksBox } from "./Webhooks";

type Outcome = { tone: "ok" | "error"; text: string } | null;

export function SettingsPage() {
  const data = useQuery({ queryKey: keys.settings, queryFn: () => settings() });
  if (data.isPending) return <Loader message="Reading the settings…" className="py-double" />;
  if (data.isError || !data.data) {
    return (
      <>
        <Header breadcrumb="Setup" title="Settings" />
        <Notice tone="error">{data.error?.message ?? "The settings could not be read."}</Notice>
      </>
    );
  }
  return <SettingsForm initial={data.data} />;
}

function SettingsForm({ initial }: { initial: ServerSettings }) {
  const queryClient = useQueryClient();
  const [file, setFile] = useState(initial.file);
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
      setFile(fresh.file);
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
        text: isApiError(error) ? error.message : "The settings could not be saved.",
      }),
  });
  const edit = (key: string, text: string) =>
    setForms((current) =>
      current.map((form) => ({
        ...form,
        fields: form.fields.map((each) => (each.key === key ? { ...each, text } : each)),
      })),
    );

  return (
    <div>
      <div className="flex items-start justify-between gap-base">
        <Header breadcrumb="Setup" title="Settings" />
        <Button
          size="sm"
          disabled={save.isPending}
          onClick={() => {
            setOutcome(null);
            save.mutate();
          }}
        >
          {save.isPending ? "Saving…" : "Save"}
        </Button>
      </div>
      <p className="m-0 mb-[12px] text-[12.5px] text-normal">
        {`The keys of ${file}. Save writes only what you changed.`}
      </p>
      <div className="flex flex-col gap-[12px]">
        <RestartBanner />
        {outcome && <Notice tone={outcome.tone}>{outcome.text}</Notice>}
        <div className="grid gap-[12px] lg:grid-cols-2">
          {forms.map((form) => (
            <Box key={form.title} title={form.title} className="self-start">
              <div className="flex flex-col gap-[12px]">
                {form.fields.map((each) => (
                  <FieldInput
                    key={each.key}
                    field={each}
                    onChange={(text) => edit(each.key, text)}
                  />
                ))}
              </div>
            </Box>
          ))}
        </div>
        <WebhooksBox />
        <Box title="This server">
          {info.data ? (
            <dl className="m-0 grid gap-x-[16px] gap-y-half [grid-template-columns:auto_1fr]">
              {[
                ["Program", info.data.program],
                ["Version", info.data.version],
                ["Address", info.data.api_address],
                ["Data directory", info.data.data_dir],
                ["Host", `${info.data.host} (${info.data.os}, ${info.data.arch})`],
              ].map(([label, value]) => (
                <div key={label} className="contents">
                  <dt className="font-mono text-[11px] text-low">{label}</dt>
                  <dd className="m-0 break-all font-mono text-[12px] text-high">{value}</dd>
                </div>
              ))}
            </dl>
          ) : (
            <p className="m-0 text-[13px] text-normal">
              {info.isError ? info.error.message : "Reading where the server runs…"}
            </p>
          )}
        </Box>
      </div>
    </div>
  );
}

/** One key as an input, with the default the server uses when the file does not set it. */
function FieldInput({ field, onChange }: { field: Field; onChange: (text: string) => void }) {
  const showsDefault = field.default !== null && field.text.trim() === "";
  return (
    <Labelled label={field.label} hint={showsDefault ? `Default ${field.default}` : undefined}>
      {field.kind === "yesno" ? (
        <select value={field.text} onChange={(e) => onChange(e.target.value)} className={CONTROL}>
          <option value="true">true</option>
          <option value="false">false</option>
        </select>
      ) : (
        <Input
          value={field.text}
          placeholder={field.default ?? ""}
          onChange={(e) => onChange(e.target.value)}
          className={field.kind === "code" ? "font-mono text-[12px]" : undefined}
        />
      )}
    </Labelled>
  );
}
