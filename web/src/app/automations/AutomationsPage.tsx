/**
 * The Automations page: recurring work that runs on its own. The list of automations on the left; on the right the
 * editor of the one chosen (its name, its schedule with a picker of common ones and its next firing, its agent, where
 * its chats work, its prompt, whether it is on, and "Run now") and its past firings, each of which opens its chat. A
 * save is `POST /v1/automations` or `PUT /v1/automations/{id}`. The event stream keeps the list and the firings current.
 */
import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { useEffect, useState } from "react";
import { Link } from "react-router";
import {
  automations,
  deleteAutomation,
  firings,
  runAutomation,
  saveAutomation,
} from "../../api/calls/automations";
import { snapshot } from "../../api/calls/board";
import { agents } from "../../api/calls/chat";
import { keys } from "../../api/keys";
import type { Automation, AutomationSettings, Firing } from "../../api/types/automations";
import { ago } from "../../chat/format";
import { Tag, type TagTone } from "../../ui/chat/Tag";
import { Button } from "../../ui/components/Button";
import { Input } from "../../ui/components/Input";
import { Loader } from "../../ui/components/Loader";
import {
  Body,
  Box,
  Chip,
  Field,
  ListCard,
  ListDetail,
  Notice,
  PageHeader,
  Select,
  sayError,
  TextArea,
} from "../kit/Kit";
import {
  type Form,
  formOf,
  newForm,
  nextLine,
  PRESETS,
  request,
  scheduleName,
  until,
} from "./model";

const NEW = "new";

/** Unix seconds, kept current once a minute so "in 3 h" does not go stale. */
function useNow(): number {
  const [now, setNow] = useState(() => Math.floor(Date.now() / 1000));
  useEffect(() => {
    const timer = setInterval(() => setNow(Math.floor(Date.now() / 1000)), 30_000);
    return () => clearInterval(timer);
  }, []);
  return now;
}

function listTag(automation: Automation): { label: string; tone: TagTone } {
  const last = automation.last_firing;
  if (last?.status === "failed") return { label: "Failed", tone: "red" };
  if (last?.status === "skipped" && automation.enabled) return { label: "Skipped", tone: "amber" };
  return automation.enabled ? { label: "On", tone: "teal" } : { label: "Off", tone: "plain" };
}

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
  const [chosen, setChosen] = useState<number | typeof NEW | null>(null);
  const [notice, setNotice] = useState<{ tone: "ok" | "error"; text: string } | null>(null);
  const choose = (next: number | typeof NEW) => {
    setNotice(null);
    setChosen(next);
  };

  const header = (
    <PageHeader
      title="Automations"
      context="Chats that start on a schedule, in the server's local time. What they offer lands in the Inbox."
      actions={
        <Button size="sm" onClick={() => choose(NEW)}>
          New automation
        </Button>
      }
    />
  );
  if (list.isPending) {
    return (
      <>
        {header}
        <Loader message="Reading the automations…" className="py-double" />
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

  return (
    <>
      {header}
      <ListDetail
        list={
          list.data.length === 0 ? (
            <Box title="Automations">
              <Body>None yet. One could triage new issues every morning.</Body>
            </Box>
          ) : (
            list.data.map((automation) => (
              <ListCard
                key={automation.id}
                title={automation.name}
                tags={[listTag(automation)]}
                line={listLine(automation, now)}
                alert={automation.last_firing?.status === "failed"}
                chosen={chosen === automation.id}
                onClick={() => choose(automation.id)}
              />
            ))
          )
        }
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
    </>
  );
}

function Editor({
  saved,
  now,
  notice,
  setNotice,
  onCreated,
  onDeleted,
}: {
  saved: Automation | undefined;
  now: number;
  notice: { tone: "ok" | "error"; text: string } | null;
  setNotice: (notice: { tone: "ok" | "error"; text: string } | null) => void;
  onCreated: (id: number) => void;
  onDeleted: () => void;
}) {
  const queryClient = useQueryClient();
  const [form, setForm] = useState<Form>(() => (saved ? formOf(saved) : newForm()));
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
          ? { tone: "ok", text: "Started; its chat is among the firings below." }
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

  return (
    <>
      <Box>
        <div className="flex flex-wrap items-center gap-base">
          <Input
            aria-label="Name of the automation"
            placeholder="Name of the automation"
            className="min-w-[200px] flex-1 font-semibold"
            value={form.name}
            onChange={(event) => edit({ name: event.target.value })}
          />
          {confirming ? (
            <span className="flex items-center gap-base text-sm text-normal">
              Delete this automation?
              <Button size="sm" variant="outline" onClick={() => setConfirming(false)}>
                Keep it
              </Button>
              <Button
                size="sm"
                variant="destructive"
                disabled={remove.isPending}
                onClick={() => remove.mutate()}
              >
                Delete
              </Button>
            </span>
          ) : (
            <>
              <Button
                size="sm"
                variant="outline"
                disabled={id === null || run.isPending}
                onClick={() => run.mutate()}
              >
                Run now
              </Button>
              {id !== null && (
                <Button size="sm" variant="outline" onClick={() => setConfirming(true)}>
                  Delete automation
                </Button>
              )}
              <Button size="sm" disabled={save.isPending} onClick={submit}>
                Save
              </Button>
            </>
          )}
        </div>
        {notice && <Notice tone={notice.tone}>{notice.text}</Notice>}
      </Box>

      <Box
        title="Schedule"
        aside={
          <span className="flex gap-half">
            <Chip active={form.enabled} onClick={() => edit({ enabled: true })}>
              On
            </Chip>
            <Chip active={!form.enabled} onClick={() => edit({ enabled: false })}>
              Off
            </Chip>
          </span>
        }
      >
        <div className="flex flex-wrap gap-double">
          <Field label="Repeats">
            {(fieldId) => (
              <Select
                id={fieldId}
                value={preset ? preset[1] : "custom"}
                onChange={(event) => {
                  if (event.target.value !== "custom") edit({ schedule: event.target.value });
                }}
              >
                {PRESETS.map(([name, cron]) => (
                  <option key={cron} value={cron}>
                    {name}
                  </option>
                ))}
                {!preset && <option value="custom">Custom</option>}
              </Select>
            )}
          </Field>
          <Field label="Cron, in the server's local time">
            {(fieldId) => (
              <Input
                id={fieldId}
                className="font-mono"
                placeholder="0 8 * * 1-5"
                value={form.schedule}
                onChange={(event) => edit({ schedule: event.target.value })}
              />
            )}
          </Field>
        </div>
        <Body>{nextLine(saved, form.schedule, form.enabled, now)}</Body>
      </Box>

      <Box title="Chat" aside="Ends once the agent has answered">
        <div className="flex flex-wrap gap-double">
          <Field label="Agent">
            {(fieldId) => (
              <Select
                id={fieldId}
                value={form.agent}
                onChange={(event) => edit({ agent: event.target.value })}
              >
                {[...names].map((name) => (
                  <option key={name} value={name}>
                    {name}
                  </option>
                ))}
              </Select>
            )}
          </Field>
          <Field label="Works in">
            {(fieldId) => (
              <Select
                id={fieldId}
                value={form.place}
                onChange={(event) => edit({ place: event.target.value as Form["place"] })}
              >
                <option value="nowhere">An empty directory</option>
                {repositories.data?.boards.map((board) => (
                  <option key={board.repository.id} value={`repository:${board.repository.id}`}>
                    {board.repository.name}
                  </option>
                ))}
                <option value="harness">Harness configuration</option>
              </Select>
            )}
          </Field>
        </div>
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
      </Box>

      <Box title="Firings" aside="Each opens its chat">
        {id !== null && past.isPending ? (
          <Body>Reading the firings…</Body>
        ) : past.data && past.data.length > 0 ? (
          <ul className="m-0 flex list-none flex-col gap-half p-0">
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
    </>
  );
}

/** A firing: when, what came of it, who fired it, and its chat, which a click opens. */
function FiringRow({ firing, now }: { firing: Firing; now: number }) {
  const tone: TagTone =
    firing.status === "started" ? "teal" : firing.status === "skipped" ? "amber" : "red";
  const what =
    firing.run_id === null
      ? firing.reason
      : firing.chat_status
        ? `Chat ${firing.run_id} · ${firing.chat_status}`
        : `Chat ${firing.run_id}`;
  const line = (
    <>
      <span className="w-[90px] shrink-0 font-mono text-xs text-low">{ago(now, firing.at)}</span>
      <Tag tone={tone}>{firing.status}</Tag>
      <span className="w-[80px] shrink-0 font-mono text-xs text-low">
        {firing.manual ? "Run now" : "Schedule"}
      </span>
      <span className="min-w-0 break-words text-sm font-medium text-high">{what}</span>
    </>
  );
  const row = "flex items-center gap-double rounded-lg border bg-surface p-base";
  return firing.run_id === null ? (
    <div className={row}>{line}</div>
  ) : (
    <Link to={`/chat/${firing.run_id}`} className={`${row} hover:bg-lane`}>
      {line}
    </Link>
  );
}
