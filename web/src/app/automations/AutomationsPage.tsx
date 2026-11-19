/**
 * The Automations page: recurring work that runs on its own. The automations are the list on the left; the one chosen
 * is the detail on the right, where its name, its schedule with a picker of common ones and its next firing, its
 * agent, where its chats work, its prompt and a switch for whether it runs are edited, and its past runs are listed, each of
 * which opens its chat. A save is `POST /v1/automations` or `PUT /v1/automations/{id}`, "Run now" is
 * `POST /v1/automations/{id}/run` and a delete `DELETE /v1/automations/{id}`. The event stream keeps the list and the
 * past runs current, so "in 3 h" and the next firing move on their own.
 *
 * The editor of the one chosen is `AutomationEditor.tsx`, its properties `AutomationProperties.tsx` and a past run
 * `FiringRow.tsx`.
 *
 * The page fills the window and each column scrolls on its own: the past runs of an automation grow without limit, and
 * reading them should not carry the list of automations away with it.
 */
import { PlusIcon } from "@phosphor-icons/react";
import { useQuery } from "@tanstack/react-query";
import { useState } from "react";
import { useSearchParams } from "react-router";
import { automations } from "../../api/calls/automations";
import { sayError } from "../../api/errorText";
import { keys } from "../../api/keys";
import type { Automation } from "../../api/types/automations";
import type { TagTone } from "../../ui/chat/Tag";
import { Button } from "../../ui/components/Button";
import { useNow } from "../../ui/lib/useNow";
import { Empty } from "../../ui/page/Empty";
import { ListCard, ListDetail } from "../../ui/page/List";
import { Notice } from "../../ui/page/Notice";
import { PageHeader } from "../../ui/page/PageHeader";
import { SkeletonCard, SkeletonList } from "../../ui/page/Skeleton";
import { Editor } from "./AutomationEditor";
import { type Form, newForm, SUGGESTED_AUTOMATIONS, scheduleName, until } from "./model";

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
