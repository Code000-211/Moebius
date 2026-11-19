/**
 * The Automations page: recurring work that runs on its own. The automations are the list on the left; the one chosen
 * is the detail on the right, where its name, its schedule with a picker of common ones and its next firing, its
 * agent, where its chats work, its prompt and a switch for whether it runs are edited, and its past runs are listed, each of
 * which opens its chat. A save is `POST /v1/automations` or `PUT /v1/automations/{id}`, "Run now" is
 * `POST /v1/automations/{id}/run` and a delete `DELETE /v1/automations/{id}`. The event stream keeps the list and the
 * past runs current, so "in 3 h" and the next firing move on their own.
 *
 * The page fills the window and each column scrolls on its own: the past runs of an automation grow without limit, and
 * reading them should not carry the list of automations away with it.
 */
import { PlusIcon } from "@phosphor-icons/react";
import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { useState } from "react";
import { Link, useSearchParams } from "react-router";
import {
  automations,
  deleteAutomation,
  firings,
  runAutomation,
  saveAutomation,
} from "../../api/calls/automations";
import { snapshot } from "../../api/calls/board";
import { agents } from "../../api/calls/chat";
import { sayError } from "../../api/errorText";
import { keys } from "../../api/keys";
import type { Automation, AutomationSettings, Firing } from "../../api/types/automations";
import type { TagTone } from "../../ui/chat/Tag";
import { Tag } from "../../ui/chat/Tag";
import { Button } from "../../ui/components/Button";
import { Input } from "../../ui/components/Input";
import { ago } from "../../ui/lib/format";
import { useNow } from "../../ui/lib/useNow";
import { Box } from "../../ui/page/Box";
import { EditableProperty } from "../../ui/page/EditableProperty";
import { Empty } from "../../ui/page/Empty";
import { Field, Select, TextArea } from "../../ui/page/Field";
import { ListCard, ListDetail } from "../../ui/page/List";
import { Notice } from "../../ui/page/Notice";
import { Body, PageHeader } from "../../ui/page/PageHeader";
import { PageProperties, Property } from "../../ui/page/Properties";
import { SkeletonCard, SkeletonLines, SkeletonList } from "../../ui/page/Skeleton";
import { Switch } from "../../ui/page/Switch";
import {
  type Form,
  formOf,
  newForm,
  nextLine,
  type Place,
  PRESETS,
  request,
  SUGGESTED_AUTOMATIONS,
  scheduleName,
  until,
} from "./model";

const NEW = "new";

/** Whether it will fire. Only a firing that went wrong or was passed over takes a hue of its own. */
function statusWord(automation: Automation): { label: string; tone: TagTone } {
  const last = automation.last_firing;
  if (last?.status === "failed") return { label: "Failed", tone: "error" };
  if (last?.status === "skipped" && automation.enabled) {
    return { label: "Skipped", tone: "attention" };
  }
  return automation.enabled ? { label: "On", tone: "plain" } : { label: "Off", tone: "plain" };
}

/** The line under a name: when it fires next, or why the last firing did not work. */
function listLine(automation: Automation, now: number): string {
  const last = automation.last_firing;
  if (last?.status === "failed") return last.reason;
  if (automation.next_firing !== null) {
    return `${scheduleName(automation.schedule)} · next ${until(now, automation.next_firing)}`;
  }
  return scheduleName(automation.schedule);
}

export function AutomationsPage() {
  const now = useNow();
  const list = useQuery({ queryKey: keys.automations, queryFn: automations });
  // The automation is in the address (`?automation=<id>`, or `new`), so a reload and a link keep it.
  const [params, setParams] = useSearchParams();
  const asked = params.get("automation");
  const chosen: number | typeof NEW | null =
    asked === "new"
      ? NEW
      : asked !== null && Number.isInteger(Number(asked))
        ? Number(asked)
        : null;
  const setChosen = (next: number | typeof NEW | null) =>
    setParams(
      (current) => {
        const query = new URLSearchParams(current);
        if (next === null) query.delete("automation");
        else query.set("automation", next === NEW ? "new" : String(next));
        return query;
      },
      { replace: true },
    );
  /** The automation the form starts from, when the page offered one. */
  const [starting, setStarting] = useState<Form | null>(null);
  const [notice, setNotice] = useState<{ tone: "ok" | "error"; text: string } | null>(null);
  const choose = (next: number | typeof NEW) => {
    setNotice(null);
    setChosen(next);
  };

  const header = (
    <PageHeader
      title="Automations"
      context="Chats that start on a schedule."
      actions={
        <Button
          size="sm"
          onClick={() => {
            setStarting(null);
            choose(NEW);
          }}
        >
          <PlusIcon className="size-4" weight="bold" aria-hidden="true" />
          New automation
        </Button>
      }
    />
  );
  if (list.isPending) {
    return (
      <>
        {header}
        <ListDetail
          list={<SkeletonList rows={4} message="Reading the automations" />}
          detail={<SkeletonCard lines={4} />}
        />
      </>
    );
  }
  if (list.error || !list.data) {
    return (
      <>
        {header}
        <Notice tone="error">{sayError(list.error, "Could not read the automations.")}</Notice>
      </>
    );
  }
  const saved =
    typeof chosen === "number" ? list.data.find((each) => each.id === chosen) : undefined;

  // With none saved there is nothing to list beside the page, so the page is one card that offers a place to start.
  if (list.data.length === 0) {
    return (
      <>
        {header}
        {chosen === NEW ? (
          <Editor
            key={NEW}
            now={now}
            notice={notice}
            setNotice={setNotice}
            starting={starting}
            onCreated={(id) => setChosen(id)}
            onDeleted={() => setChosen(null)}
          />
        ) : (
          <Empty
            title="No automation yet"
            body="An automation starts a chat on a schedule. Pick a starting point, or write your own."
            action={
              <div className="flex flex-wrap gap-2">
                {SUGGESTED_AUTOMATIONS.map((each) => (
                  <Button
                    key={each.name}
                    size="sm"
                    variant="secondary"
                    onClick={() => {
                      setStarting({ ...newForm(), ...each });
                      choose(NEW);
                    }}
                  >
                    {each.name}
                  </Button>
                ))}
              </div>
            }
          />
        )}
      </>
    );
  }

  return (
    <div className="flex h-full min-h-0 flex-col">
      {header}
      <ListDetail
        list={list.data.map((automation) => (
          <ListCard
            key={automation.id}
            title={automation.name}
            tags={[statusWord(automation)]}
            line={listLine(automation, now)}
            alert={automation.last_firing?.status === "failed"}
            selected={chosen === automation.id}
            onClick={() => choose(automation.id)}
          />
        ))}
        detail={
          chosen === NEW || saved ? (
            <Editor
              key={saved?.id ?? NEW}
              saved={saved}
              now={now}
              notice={notice}
              setNotice={setNotice}
              onCreated={(id) => setChosen(id)}
              onDeleted={() => setChosen(null)}
            />
          ) : null
        }
      />
    </div>
  );
}

/** The editor of one automation: what it is called, when it runs, what its chats are, and what came of them. */
function Editor({
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

/** A firing: what came of it and its status word, then when it ran and what asked for it. */
function FiringRow({ firing, now }: { firing: Firing; now: number }) {
  const tone: TagTone =
    firing.status === "started" ? "progress" : firing.status === "skipped" ? "attention" : "error";
  const what =
    firing.run_id === null
      ? firing.reason
      : firing.chat_status
        ? `Chat ${firing.run_id} · ${firing.chat_status}`
        : `Chat ${firing.run_id}`;
  const body = (
    <>
      <span className="min-w-0 break-words text-base text-text">{what}</span>
      <span className="flex flex-wrap items-center gap-2">
        <Tag tone={tone}>{firing.status}</Tag>
        <span className="text-sm text-secondary tabular-nums">{ago(now, firing.at)}</span>
        <span className="text-sm text-secondary">{firing.manual ? "Run now" : "Schedule"}</span>
      </span>
    </>
  );
  const row =
    "flex min-h-8 flex-col gap-2 rounded-control bg-sunken p-4 transition-colors duration-(--motion-routine) ease-out";
  return firing.run_id === null ? (
    <div className={row}>{body}</div>
  ) : (
    <Link to={`/chat/${firing.run_id}`} className={`${row} hover:bg-raised`}>
      {body}
    </Link>
  );
}

type Props = {
  places: Array<[string, string]>;
  agentNames: string[];
  now: number;
};

/** Every schedule the picker offers, and "Custom" for one it does not. */
function repeatOptions(schedule: string): Array<[string, string]> {
  const preset = PRESETS.find(([, cron]) => cron === schedule.trim());
  return [
    ...PRESETS.map(([name, cron]) => [cron, name] as [string, string]),
    ...(preset ? [] : [[schedule.trim(), "Custom"] as [string, string]]),
  ];
}

/**
 * The properties of an automation that exists, each edited where it stands and saved alone: its name, whether it runs on
 * its schedule, the schedule, its agent and where its chats work. A save sends the automation as it is stored with that
 * one change, and the list is read again once the server answered.
 */
function SavedProperties({
  saved,
  places,
  agentNames,
  now,
  onChanged,
  onRefused,
}: Props & {
  saved: Automation;
  onChanged: (patch: Partial<Form>) => void;
  onRefused: () => void;
}) {
  const queryClient = useQueryClient();
  const save = async (patch: Partial<Form>) => {
    onRefused();
    const made = request({ ...formOf(saved), ...patch });
    if ("problem" in made) throw new Error(made.problem);
    await saveAutomation(saved.id, made.ok);
    onChanged(patch);
    await queryClient.invalidateQueries({ queryKey: keys.automations });
  };
  const stored = formOf(saved);
  const place = places.find(([value]) => value === stored.place)?.[1] ?? stored.place;
  return (
    <>
      <EditableProperty
        name="Name"
        editor={{ kind: "text", value: stored.name, onSave: (name) => save({ name }) }}
      >
        {stored.name}
      </EditableProperty>
      <EditableProperty
        name="Runs"
        editor={{
          kind: "choice",
          options: [
            ["on", "On its schedule"],
            ["off", "Only when run now"],
          ],
          value: stored.enabled ? "on" : "off",
          onSave: (value) => save({ enabled: value === "on" }),
        }}
      >
        {stored.enabled ? "On its schedule" : "Only when run now"}
      </EditableProperty>
      <EditableProperty
        name="Repeats"
        editor={{
          kind: "choice",
          options: repeatOptions(stored.schedule),
          value: stored.schedule.trim(),
          onSave: (schedule) => save({ schedule }),
        }}
      >
        {scheduleName(stored.schedule)}
      </EditableProperty>
      <EditableProperty
        name="Cron"
        editor={{ kind: "text", value: stored.schedule, onSave: (schedule) => save({ schedule }) }}
      >
        <span className="font-mono text-sm">{stored.schedule}</span>
      </EditableProperty>
      <Property name="Next">
        <span className="tabular-nums">
          {nextLine(saved, stored.schedule, stored.enabled, now)}
        </span>
      </Property>
      <EditableProperty
        name="Agent"
        editor={{
          kind: "choice",
          options: agentNames.map((name) => [name, name] as [string, string]),
          value: stored.agent,
          onSave: (agent) => save({ agent }),
        }}
      >
        {stored.agent}
      </EditableProperty>
      <EditableProperty
        name="Works in"
        editor={{
          kind: "choice",
          options: places,
          value: stored.place,
          onSave: (value) => save({ place: value as Place }),
        }}
      >
        {place}
      </EditableProperty>
    </>
  );
}

/** The properties of an automation not yet saved: the same fields, held by the editor until Save makes it. */
function NewProperties({
  form,
  edit,
  places,
  agentNames,
  preset,
  now,
}: Props & {
  form: Form;
  edit: (change: Partial<Form>) => void;
  preset: readonly [string, string] | undefined;
}) {
  return (
    <div className="flex flex-col gap-4 px-1 py-2">
      <Switch
        checked={form.enabled}
        onChange={(next) => edit({ enabled: next })}
        label="Runs on its schedule"
      />
      <Field label="Repeats">
        {(fieldId) => (
          <Select
            id={fieldId}
            options={repeatOptions(form.schedule).map(([value, name]) =>
              !preset && name === "Custom" ? (["custom", name] as [string, string]) : [value, name],
            )}
            value={preset ? preset[1] : "custom"}
            onChange={(picked) => {
              if (picked !== "custom") edit({ schedule: picked });
            }}
          />
        )}
      </Field>
      <Field
        label="Cron, in the server's local time"
        hint="Five fields: minute, hour, day of the month, month, day of the week."
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
      <Body>
        <span className="tabular-nums">
          {nextLine(undefined, form.schedule, form.enabled, now)}
        </span>
      </Body>
      <Field label="Agent">
        {(fieldId) => (
          <Select
            id={fieldId}
            options={agentNames}
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
    </div>
  );
}
