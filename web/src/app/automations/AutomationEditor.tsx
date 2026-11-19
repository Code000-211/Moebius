/**
 * The editor of one automation, the detail of the Automations page: its name, its properties (see
 * `AutomationProperties.tsx`), its prompt and its past runs, with Run now, Delete and Save. A save is
 * `POST /v1/automations` or `PUT /v1/automations/{id}`, "Run now" is `POST /v1/automations/{id}/run` and a delete
 * `DELETE /v1/automations/{id}`.
 */
import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { useState } from "react";
import {
  deleteAutomation,
  firings,
  runAutomation,
  saveAutomation,
} from "../../api/calls/automations";
import { snapshot } from "../../api/calls/board";
import { agents } from "../../api/calls/chat";
import { sayError } from "../../api/errorText";
import { keys } from "../../api/keys";
import type { Automation, AutomationSettings } from "../../api/types/automations";
import { Button } from "../../ui/components/Button";
import { Input } from "../../ui/components/Input";
import { Box } from "../../ui/page/Box";
import { Field, TextArea } from "../../ui/page/Field";
import { Notice } from "../../ui/page/Notice";
import { Body } from "../../ui/page/PageHeader";
import { PageProperties } from "../../ui/page/Properties";
import { SkeletonLines } from "../../ui/page/Skeleton";
import { NewProperties, SavedProperties } from "./AutomationProperties";
import { FiringRow } from "./FiringRow";
import { type Form, formOf, newForm, PRESETS, request } from "./model";

/** The editor of one automation: what it is called, when it runs, what its chats are, and what came of them. */
export function Editor({
  saved,
  now,
  notice,
  setNotice,
  onCreated,
  onDeleted,
  starting,
}: {
  saved?: Automation;
  /** The fields to open a new automation with, when the empty page offered one. */
  starting?: Form | null;
  now: number;
  notice: { tone: "ok" | "error"; text: string } | null;
  setNotice: (notice: { tone: "ok" | "error"; text: string } | null) => void;
  onCreated: (id: number) => void;
  onDeleted: () => void;
}) {
  const queryClient = useQueryClient();
  const [form, setForm] = useState<Form>(() => (saved ? formOf(saved) : (starting ?? newForm())));
  const [confirming, setConfirming] = useState(false);
  const id = saved?.id ?? null;
  const edit = (change: Partial<Form>) => {
    setNotice(null);
    setForm({ ...form, ...change });
  };
  const refresh = () => queryClient.invalidateQueries({ queryKey: keys.automations });
  const agentList = useQuery({ queryKey: keys.agents, queryFn: agents });
  const repositories = useQuery({ queryKey: keys.snapshot, queryFn: () => snapshot() });
  const past = useQuery({
    queryKey: keys.firings(id ?? 0),
    queryFn: () => firings(id as number),
    enabled: id !== null,
  });

  const save = useMutation({
    mutationFn: (settings: AutomationSettings) => saveAutomation(id, settings),
    onSuccess: async (automation) => {
      await refresh();
      setNotice({ tone: "ok", text: "Saved." });
      if (id === null) onCreated(automation.id);
    },
    onError: (error) => setNotice({ tone: "error", text: sayError(error, "Could not save.") }),
  });
  const run = useMutation({
    mutationFn: () => runAutomation(id as number),
    onSuccess: async (firing) => {
      await refresh();
      setNotice(
        firing.status === "started"
          ? { tone: "ok", text: "Started; its chat is among the past runs below." }
          : { tone: "error", text: `Not started: ${firing.reason}` },
      );
    },
    onError: (error) => setNotice({ tone: "error", text: sayError(error, "Could not run it.") }),
  });
  const remove = useMutation({
    mutationFn: () => deleteAutomation(id as number),
    onSuccess: async () => {
      await refresh();
      onDeleted();
    },
    onError: (error) => {
      setConfirming(false);
      setNotice({ tone: "error", text: sayError(error, "Could not delete it.") });
    },
  });

  const submit = () => {
    const made = request(form);
    if ("problem" in made) setNotice({ tone: "error", text: made.problem });
    else save.mutate(made.ok);
  };

  const names = new Set(agentList.data?.map((agent) => agent.name) ?? []);
  names.add(form.agent);
  const preset = PRESETS.find(([, cron]) => cron === form.schedule.trim());
  const places: Array<[string, string]> = [
    ["nowhere", "An empty directory"],
    ...(repositories.data?.boards.map(
      (board) => [`repository:${board.repository.id}`, board.repository.name] as [string, string],
    ) ?? []),
    ["harness", "Harness configuration"],
  ];
  // An automation that exists has nothing to save until a field of it changes; a new one can be saved as it was
  // offered, because there is no stored form to compare it with.
  const dirty = saved === undefined || JSON.stringify(form) !== JSON.stringify(formOf(saved));

  return (
    <div className="flex min-w-0 flex-col gap-6">
      <Box
        title="Automation"
        actions={
          confirming ? undefined : (
            <>
              <Button
                size="sm"
                variant="secondary"
                disabled={id === null || run.isPending}
                title={id === null ? "Save it first" : undefined}
                onClick={() => run.mutate()}
              >
                {run.isPending ? "Starting" : "Run now"}
              </Button>
              {id !== null && (
                <Button size="sm" variant="secondary" onClick={() => setConfirming(true)}>
                  Delete automation
                </Button>
              )}
              <Button size="sm" disabled={save.isPending || !dirty} onClick={submit}>
                {save.isPending ? "Saving" : "Save"}
              </Button>
            </>
          )
        }
      >
        <div className="flex flex-col gap-4">
          {!saved && (
            <Field label="Name of the automation">
              {(fieldId) => (
                <Input
                  id={fieldId}
                  placeholder="Morning triage"
                  value={form.name}
                  onChange={(event) => edit({ name: event.target.value })}
                />
              )}
            </Field>
          )}
          {notice && <Notice tone={notice.tone}>{notice.text}</Notice>}
          {confirming && (
            <div className="flex flex-col gap-4 rounded-control bg-sunken p-4 sm:flex-row sm:items-center">
              <p className="m-0 flex min-h-8 min-w-0 flex-1 items-center break-words text-base text-text">
                Delete this automation? Its past runs stay listed.
              </p>
              <div className="flex gap-2">
                <Button size="sm" variant="secondary" onClick={() => setConfirming(false)}>
                  Keep it
                </Button>
                <Button
                  size="sm"
                  variant="danger"
                  disabled={remove.isPending}
                  onClick={() => remove.mutate()}
                >
                  Delete
                </Button>
              </div>
            </div>
          )}
        </div>
      </Box>

      <PageProperties label={`Properties of ${saved?.name ?? "the new automation"}`}>
        {saved ? (
          <SavedProperties
            saved={saved}
            places={places}
            agentNames={[...names]}
            now={now}
            onChanged={(patch) => setForm((held) => ({ ...held, ...patch }))}
            onRefused={() => setNotice(null)}
          />
        ) : (
          <NewProperties
            form={form}
            edit={edit}
            places={places}
            agentNames={[...names]}
            preset={preset}
            now={now}
          />
        )}
      </PageProperties>

      <Box title="Chat" aside="Ends once the agent has answered">
        <div className="flex flex-col gap-4">
          <Field label="Prompt">
            {(fieldId) => (
              <TextArea
                id={fieldId}
                mono
                rows={6}
                placeholder="Triage the issues opened since yesterday: label them and say which are ready."
                value={form.prompt}
                onChange={(event) => edit({ prompt: event.target.value })}
              />
            )}
          </Field>
        </div>
      </Box>

      <Box title="Past runs" className="min-w-0">
        {id !== null && past.isPending ? (
          <SkeletonLines lines={3} message="Reading the past runs" />
        ) : past.data && past.data.length > 0 ? (
          <ul className="m-0 flex list-none flex-col gap-2 p-0">
            {past.data.map((firing) => (
              <li key={firing.id}>
                <FiringRow firing={firing} now={now} />
              </li>
            ))}
          </ul>
        ) : (
          <Body>It has not fired yet.</Body>
        )}
      </Box>
    </div>
  );
}
