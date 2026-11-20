/**
 * The Automations page: recurring work that runs on its own, as one list of 36px rows: the name, when it fires next
 * (or why its last firing did not work), and at the right a word where something is off. A row opens the automation in
 * the editor window (`AutomationEditor.tsx`): its name, its schedule with a picker of common ones, its agent, where its
 * chats work, its prompt and a switch for whether it runs, and its past runs, each of which opens its chat. "New
 * automation" opens the same window empty. The chosen automation is in the address (`?automation=<id>`, or `new`), so a
 * reload and a link keep it open; closing the window takes it out. The event stream keeps the list current, so "in 3 h"
 * moves on its own.
 */
import { PlusIcon } from "@phosphor-icons/react";
import { useQuery } from "@tanstack/react-query";
import { useState } from "react";
import { useSearchParams } from "react-router";
import { automations } from "../../api/calls/automations";
import { keys } from "../../api/keys";
import type { Automation } from "../../api/types/automations";
import { Button } from "../../ui/components/Button";
import { cn } from "../../ui/lib/cn";
import { useNow } from "../../ui/lib/useNow";
import { Empty } from "../../ui/page/Empty";
import { PageHeader } from "../../ui/page/PageHeader";
import { ReadFailed } from "../../ui/page/ReadFailed";
import { SkeletonList } from "../../ui/page/Skeleton";
import { Editor } from "./AutomationEditor";
import { type Form, newForm, SUGGESTED_AUTOMATIONS, scheduleName, until } from "./model";

const NEW = "new";

/** The one word at the right of a row, where the automation is not simply on. */
function statusWord(automation: Automation): { text: string; className: string } | null {
  const last = automation.last_firing;
  if (last?.status === "failed") return { text: "Failed", className: "text-error-text" };
  if (last?.status === "skipped" && automation.enabled)
    return { text: "Skipped", className: "text-attention-text" };
  return automation.enabled ? null : { text: "Off", className: "text-muted" };
}

/** The line after a name: when it fires next, or why the last firing did not work. */
function listLine(automation: Automation, now: number): string {
  const last = automation.last_firing;
  if (last?.status === "failed") return last.reason;
  if (automation.next_firing !== null) {
    return `${scheduleName(automation.schedule)} · next ${until(now, automation.next_firing)}`;
  }
  return scheduleName(automation.schedule);
}

function AutomationRow({
  automation,
  now,
  onOpen,
}: {
  automation: Automation;
  now: number;
  onOpen: () => void;
}) {
  const state = statusWord(automation);
  return (
    <button
      type="button"
      data-testid="automation-row"
      aria-label={`Edit automation ${automation.name}`}
      onClick={onOpen}
      className="flex h-9 w-full cursor-pointer items-center gap-2.5 border-b border-hairline px-4 text-left text-base text-text hover:bg-row-hover"
    >
      <span className="shrink-0 truncate font-medium" style={{ maxWidth: "16rem" }}>
        {automation.name}
      </span>
      <span
        className={cn(
          "min-w-0 flex-1 truncate",
          automation.last_firing?.status === "failed" ? "text-error-text" : "text-muted",
        )}
      >
        {listLine(automation, now)}
      </span>
      {state && (
        <span className={cn("ml-auto shrink-0 text-xs", state.className)}>{state.text}</span>
      )}
    </button>
  );
}

export function AutomationsPage() {
  const now = useNow();
  const list = useQuery({ queryKey: keys.automations, queryFn: automations });
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
  const startNew = (from: Form | null) => {
    setStarting(from);
    setChosen(NEW);
  };
  const saved =
    typeof chosen === "number" ? list.data?.find((each) => each.id === chosen) : undefined;

  return (
    <div className="flex h-full min-h-0 flex-col">
      <PageHeader
        title="Automations"
        actions={
          <Button size="sm" onClick={() => startNew(null)}>
            <PlusIcon className="size-4" weight="bold" aria-hidden="true" />
            New automation
          </Button>
        }
      />
      <div className="min-h-0 flex-1 overflow-y-auto">
        {list.isPending ? (
          <SkeletonList rows={4} message="Reading the automations" />
        ) : list.isError ? (
          <ReadFailed
            error={list.error}
            fallback="Could not read the automations."
            busy={list.isFetching}
            onRetry={() => void list.refetch()}
          />
        ) : list.data.length === 0 ? (
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
                    onClick={() => startNew({ ...newForm(), ...each })}
                  >
                    {each.name}
                  </Button>
                ))}
              </div>
            }
          />
        ) : (
          list.data.map((automation) => (
            <AutomationRow
              key={automation.id}
              automation={automation}
              now={now}
              onOpen={() => setChosen(automation.id)}
            />
          ))
        )}
      </div>
      {chosen === NEW || saved ? (
        <Editor
          key={saved?.id ?? NEW}
          saved={saved}
          starting={starting}
          now={now}
          onClose={() => setChosen(null)}
        />
      ) : null}
    </div>
  );
}
