/**
 * The editor of one automation, in the editor window (`EditorWindow`, the one the agents and rules open in): its name,
 * whether it runs on its schedule, the schedule with a picker of common ones and when it fires next, its agent, where
 * its chats work and its prompt, and for one that exists its past runs. Save and Cancel are the window's; "Run now" and
 * Delete (which asks first) are at the left of the foot. A save is `POST /v1/automations` or `PUT /v1/automations/{id}`,
 * "Run now" is `POST /v1/automations/{id}/run` and a delete `DELETE /v1/automations/{id}`; each is told by a toast, and a
 * refusal of a save stays in the window beside the buttons.
 */
import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { useState } from "react";
import {
  deleteAutomation,
  firings,
  runAutomation,
  saveAutomation,
} from "../../api/calls/automations";
import { agents } from "../../api/calls/chat";
import { sayError } from "../../api/errorText";
import { keys } from "../../api/keys";
import type { Automation, AutomationSettings } from "../../api/types/automations";
import type { Project } from "../../api/types/projects";
import { Button } from "../../ui/components/Button";
import { DeleteButton, EditorWindow } from "../../ui/components/EditorWindow";
import { Input } from "../../ui/components/Input";
import { ConfirmDelete } from "../../ui/page/ConfirmDelete";
import { Field, Select, TextArea } from "../../ui/page/Field";
import { SkeletonLines } from "../../ui/page/Skeleton";
import { Switch } from "../../ui/page/Switch";
import { useToast } from "../../ui/page/Toast";
import { FiringRow } from "./FiringRow";
import { type Form, formOf, newForm, nextLine, type Place, PRESETS, request } from "./model";

/** Every schedule the picker offers, and "Custom" for one it does not. */
function repeatOptions(schedule: string): Array<[string, string]> {
  const preset = PRESETS.find(([, cron]) => cron === schedule.trim());
  return [
    ...PRESETS.map(([name, cron]) => [cron, name] as [string, string]),
    ...(preset ? [] : [["custom", "Custom"] as [string, string]]),
  ];
}

export function Editor({
  project,
  saved,
  starting,
  now,
  onClose,
}: {
  /** The project the automation belongs to: it works in one of the project's repositories. */
  project: Project;
  /** The automation to edit; none for a new one. */
  saved?: Automation;
  /** The fields to open a new automation with, when the empty page offered one. */
  starting?: Form | null;
  now: number;
  /** The window closes: after a save or a delete, or when the change is dropped or there is none. */
  onClose: () => void;
}) {
  const queryClient = useQueryClient();
  const toast = useToast();
  const [form, setForm] = useState<Form>(() => (saved ? formOf(saved) : (starting ?? newForm())));
  const [deleting, setDeleting] = useState(false);
  // Set once the automation is deleted, so its past runs are not asked for again.
  const [gone, setGone] = useState(false);
  // What the server refused on the last save, shown beside the buttons until the next change.
  const [refused, setRefused] = useState<string | null>(null);
  const id = saved?.id ?? null;
  const edit = (change: Partial<Form>) => {
    setRefused(null);
    setForm({ ...form, ...change });
  };
  const refresh = () => queryClient.invalidateQueries({ queryKey: keys.automations });
  const agentList = useQuery({ queryKey: keys.agents, queryFn: agents });
  const save = useMutation({
    mutationFn: (settings: AutomationSettings) => saveAutomation(id, settings),
    onSuccess: async () => {
      await refresh();
      toast({ kind: "confirmation", text: "Saved." });
      onClose();
    },
    onError: (error) => setRefused(sayError(error, "Could not save.")),
  });
  const run = useMutation({
    mutationFn: () => runAutomation(id as number),
    onSuccess: async (firing) => {
      await refresh();
      await queryClient.invalidateQueries({ queryKey: keys.firings(id as number) });
      toast(
        firing.status === "started"
          ? { kind: "confirmation", text: "Started; its chat is among the past runs." }
          : { kind: "error", text: `Not started: ${firing.reason}` },
      );
    },
    onError: (error) => toast({ kind: "error", text: sayError(error, "Could not run it.") }),
  });
  const remove = async () => {
    await deleteAutomation(id as number);
    setGone(true);
    // Its past runs are no longer read: the server has no list for an automation that is gone.
    queryClient.removeQueries({ queryKey: keys.firings(id as number) });
    await refresh();
    toast({ kind: "confirmation", text: "Deleted." });
    onClose();
  };

  const past = useQuery({
    queryKey: keys.firings(id ?? 0),
    queryFn: () => firings(id as number),
    // Not once it is deleted: the list of a gone automation is a 404.
    enabled: id !== null && !gone,
  });

  const submit = () => {
    const made = request(form, project.id);
    if ("problem" in made) setRefused(made.problem);
    else save.mutate(made.ok);
  };

  const names = new Set(agentList.data?.map((agent) => agent.name) ?? []);
  names.add(form.agent);
  const preset = PRESETS.find(([, cron]) => cron === form.schedule.trim());
  const places: Array<[string, string]> = [
    ["nowhere", "An empty directory"],
    ...project.repositories.map((each) => [`repository:${each.id}`, each.name] as [string, string]),
    ["harness", "Harness configuration"],
  ];
  // What would be lost on closing: a change to the stored automation, or anything typed into a new one.
  const dirty = JSON.stringify(form) !== JSON.stringify(saved ? formOf(saved) : newForm());

  return (
    <>
      <EditorWindow
        open
        title={saved ? `Edit scheduled workflow ${saved.name}` : "New scheduled workflow"}
        dirty={dirty}
        saving={save.isPending}
        error={refused}
        onSave={submit}
        onClose={onClose}
        actions={
          saved && (
            <>
              <Button
                size="sm"
                variant="secondary"
                disabled={run.isPending}
                onClick={() => run.mutate()}
              >
                {run.isPending ? "Starting" : "Run now"}
              </Button>
              <DeleteButton onClick={() => setDeleting(true)}>Delete automation</DeleteButton>
            </>
          )
        }
      >
        <Field label="Name">
          {(fieldId) => (
            <Input
              id={fieldId}
              placeholder="Morning triage"
              value={form.name}
              onChange={(event) => edit({ name: event.target.value })}
            />
          )}
        </Field>
        <Switch
          checked={form.enabled}
          onChange={(next) => edit({ enabled: next })}
          label="Runs on its schedule"
        />
        <Field label="Repeats">
          {(fieldId) => (
            <Select
              id={fieldId}
              options={repeatOptions(form.schedule)}
              value={preset ? preset[1] : "custom"}
              onChange={(picked) => {
                if (picked !== "custom") edit({ schedule: picked });
              }}
            />
          )}
        </Field>
        <Field
          label="Cron, in the server's local time"
          hint={nextLine(saved, form.schedule, form.enabled, now)}
        >
          {(fieldId) => (
            <Input
              id={fieldId}
              className="font-mono text-sm"
              placeholder="0 8 * * 1-5"
              value={form.schedule}
              onChange={(event) => edit({ schedule: event.target.value })}
            />
          )}
        </Field>
        <Field label="Agent">
          {(fieldId) => (
            <Select
              id={fieldId}
              options={[...names]}
              value={form.agent}
              onChange={(agent) => edit({ agent })}
            />
          )}
        </Field>
        <Field label="Works in">
          {(fieldId) => (
            <Select
              id={fieldId}
              options={places}
              value={form.place}
              onChange={(picked) => edit({ place: picked as Place })}
            />
          )}
        </Field>
        <Field label="Prompt" hint="Its chat ends once the agent has answered.">
          {(fieldId) => (
            <TextArea
              id={fieldId}
              mono
              rows={5}
              placeholder="Triage the issues opened since yesterday: label them and say which are ready."
              value={form.prompt}
              onChange={(event) => edit({ prompt: event.target.value })}
            />
          )}
        </Field>
        {id !== null && (
          <section aria-label="Past runs" className="flex flex-col">
            <h3 className="m-0 mb-1 text-base font-semibold text-text">Past runs</h3>
            {past.isPending ? (
              <SkeletonLines lines={3} message="Reading the past runs" />
            ) : past.data && past.data.length > 0 ? (
              <ul className="m-0 flex list-none flex-col p-0">
                {past.data.map((firing) => (
                  <li key={firing.id}>
                    <FiringRow firing={firing} now={now} />
                  </li>
                ))}
              </ul>
            ) : (
              <p className="m-0 text-sm text-muted">It has not fired yet.</p>
            )}
          </section>
        )}
      </EditorWindow>
      <ConfirmDelete
        open={deleting}
        onOpenChange={setDeleting}
        title="Delete this automation?"
        confirmLabel="Delete automation"
        onConfirm={remove}
      >
        The automation {saved?.name} is removed. Its past runs stay listed.
      </ConfirmDelete>
    </>
  );
}
