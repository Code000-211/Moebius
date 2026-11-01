/**
 * One item of the inbox: why it is here in a word and a sentence, what it shows (the facts of a change, the trials or
 * the diff of a proposal, the issues of a grilling to edit, a report) and the buttons that act on it. The card is the
 * only layer on this page, so the kind's colour sits on the word alone and the parts are apart by space. While a
 * request is on its way the buttons give way to what it does; a refusal of the server stays under the item in the
 * server's own words. The item leaves when the snapshot is read again, which the answer and the event cause.
 */
import { useMutation, useQueryClient } from "@tanstack/react-query";
import { useState } from "react";
import { useNavigate } from "react-router";
import { isApiError } from "../../api/http";
import { keys } from "../../api/keys";
import type { InboxItem, Snapshot } from "../../api/types/board";
import { CodeBlock } from "../../ui/chat/CodeBlock";
import { Tag } from "../../ui/chat/Tag";
import { Button } from "../../ui/components/Button";
import { Input } from "../../ui/components/Input";
import { cn } from "../../ui/lib/cn";
import { ago } from "../../ui/lib/format";
import { Field, TextArea } from "../../ui/page/Field";
import { Notice } from "../../ui/page/Notice";
import { Status } from "../../ui/page/Status";
import { type Action, perform, working } from "./actions";
import {
  APPROVE_ISSUE,
  CHANGE_CLOSED,
  changeOf,
  chatOf,
  confirmable,
  type Draft,
  draftsOf,
  facts,
  found,
  GRILLING_ISSUES,
  GRILLING_REPORT,
  grillingOf,
  issuesSent,
  kindTag,
  meta,
  PROPOSAL,
  proposalOf,
  READY_TO_MERGE,
  removeIssue,
  STUCK,
  TAKEOVER,
  takeoverOf,
  trialsOf,
  why,
} from "./model";

export function ItemCard({
  item,
  snapshot,
  now,
}: {
  item: InboxItem;
  snapshot: Snapshot;
  now: number;
}) {
  const queryClient = useQueryClient();
  const navigate = useNavigate();
  const about = found(snapshot, item);
  const { label, tone } = kindTag(item);
  const [rejecting, setRejecting] = useState<string | null>(null);
  const [drafts, setDrafts] = useState<Draft[]>(() =>
    item.kind === GRILLING_ISSUES ? draftsOf(item) : [],
  );
  const [notice, setNotice] = useState<string | null>(null);
  const mutation = useMutation({
    mutationFn: perform,
    onSuccess: () => {
      setNotice(null);
      setRejecting(null);
    },
    onError: (error) =>
      setNotice(isApiError(error) ? error.message : "The request could not be made. Try again."),
    onSettled: () => {
      void queryClient.invalidateQueries({ queryKey: keys.snapshot });
      void queryClient.invalidateQueries({ queryKey: keys.cards });
    },
  });
  const act = (action: Action) => {
    if (mutation.isPending) return;
    setNotice(null);
    mutation.mutate(action);
  };

  const sentence = why(item, about);
  const change = changeOf(item);
  const grilling = grillingOf(item);
  const proposal = proposalOf(item);
  const takeover = takeoverOf(item);
  const reasonReady = rejecting !== null && rejecting.trim() !== "";

  const footer = () => {
    if (mutation.isPending && mutation.variables) {
      return <p className="m-0 text-sm text-secondary">{working(mutation.variables)}</p>;
    }
    if (rejecting !== null && change !== null) {
      const send = () => act({ kind: "reject", change, reason: rejecting.trim() });
      return (
        <div className="flex flex-wrap items-end gap-3">
          <Field label="Why? The next round is told" className="min-w-60 flex-1">
            {(id) => (
              <Input
                id={id}
                value={rejecting}
                onChange={(e) => setRejecting(e.target.value)}
                onCommandEnter={() => reasonReady && send()}
                autoFocus
              />
            )}
          </Field>
          <Button size="sm" disabled={!reasonReady} onClick={send}>
            Send back
          </Button>
          <Button size="sm" variant="secondary" onClick={() => setRejecting(null)}>
            Cancel
          </Button>
        </div>
      );
    }
    const buttons: React.ReactNode[] = [];
    const primary = (name: string, action: Action | null, disabled = false) => (
      <Button
        key={name}
        size="sm"
        disabled={disabled || action === null}
        onClick={() => action && act(action)}
      >
        {name}
      </Button>
    );
    const plain = (name: string, onClick: (() => void) | null) => (
      <Button
        key={name}
        size="sm"
        variant="secondary"
        disabled={onClick === null}
        onClick={() => onClick?.()}
      >
        {name}
      </Button>
    );
    if (item.kind === READY_TO_MERGE && change !== null) {
      buttons.push(primary("Merge", { kind: "merge", change }));
      if (item.card_id !== null) {
        const board = about.board;
        buttons.push(
          plain(
            "Open live view",
            board ? () => navigate(`/board/${board.repository.id}?card=${item.card_id}`) : null,
          ),
        );
        buttons.push(plain("Reject with a reason", () => setRejecting("")));
      } else {
        const chat = chatOf(item);
        buttons.push(plain("Open chat", chat === null ? null : () => navigate(`/chat/${chat}`)));
      }
    } else if (item.kind === APPROVE_ISSUE && item.card_id !== null) {
      buttons.push(primary("Approve", { kind: "approve", card: item.card_id }));
    } else if ((item.kind === STUCK || item.kind === CHANGE_CLOSED) && item.card_id !== null) {
      buttons.push(primary("Retry", { kind: "retry", card: item.card_id }));
    } else if (item.kind === PROPOSAL && proposal !== null) {
      buttons.push(primary("Merge", { kind: "mergeProposal", proposal }));
      buttons.push(plain("Dismiss", () => act({ kind: "dismissProposal", proposal })));
    } else if (item.kind === TAKEOVER && takeover !== null) {
      buttons.push(primary("Merge", { kind: "mergeTakeover", takeover }));
      buttons.push(plain("Dismiss", () => act({ kind: "dismissTakeover", takeover })));
    } else if (item.kind === GRILLING_ISSUES && grilling !== null) {
      buttons.push(
        primary(
          "Confirm",
          { kind: "confirmGrilling", grilling, issues: issuesSent(drafts) },
          !confirmable(drafts),
        ),
      );
      buttons.push(plain("Dismiss", () => act({ kind: "dismissGrilling", grilling })));
    } else if (item.kind === GRILLING_REPORT && grilling !== null) {
      buttons.push(primary("Dismiss", { kind: "dismissGrilling", grilling }));
    }
    return <div className="flex flex-wrap gap-2">{buttons}</div>;
  };

  return (
    <article
      data-testid="inbox-item"
      data-kind={item.kind}
      className="flex flex-col gap-4 rounded-card bg-surface p-4 shadow-card"
    >
      <div className="flex flex-wrap items-center gap-3">
        <Status tone={tone}>{label}</Status>
        <span className="min-w-0 break-words text-sm text-secondary">{meta(item, about)}</span>
        <span className="flex-1" />
        <span className="text-sm text-secondary tabular-nums">{ago(now, item.at)}</span>
      </div>

      <div className="flex flex-col gap-2">
        <h2 className="m-0 break-words text-lg font-semibold tracking-tight text-text">
          {item.title}
        </h2>
        {sentence && (
          <p className="m-0 max-w-prose text-base text-secondary [overflow-wrap:anywhere]">
            {sentence}
          </p>
        )}
      </div>

      {item.kind === READY_TO_MERGE && <Facts item={item} snapshot={snapshot} />}
      {item.kind === PROPOSAL && typeof item.data.diff === "string" ? (
        <OwnChange item={item} />
      ) : (
        item.kind === PROPOSAL && <Trials item={item} />
      )}
      {item.kind === GRILLING_ISSUES && <Issues drafts={drafts} onChange={setDrafts} />}
      {item.kind === GRILLING_REPORT && (
        <div className="whitespace-pre-wrap break-words rounded-control bg-sunken p-4 text-base text-text">
          {typeof item.data.report === "string" ? item.data.report : ""}
        </div>
      )}
      {footer()}
      {notice && <Notice tone="error">{notice}</Notice>}
    </article>
  );
}

function Facts({ item, snapshot }: { item: InboxItem; snapshot: Snapshot }) {
  return (
    <dl className="m-0 flex flex-wrap gap-2">
      {facts(item, found(snapshot, item)).map((fact) => (
        <div key={fact.label} className="min-w-40 flex-1 rounded-control bg-sunken p-3">
          <dt className="text-sm font-semibold text-secondary">{fact.label}</dt>
          <dd
            className={cn(
              "m-0 mt-1 break-all text-base tabular-nums",
              fact.good ? "text-complete-text" : "text-text",
              fact.mono && "font-mono text-sm",
            )}
          >
            {fact.value}
          </dd>
        </div>
      ))}
    </dl>
  );
}

/** A proposal for Moebius's own configuration: the Retro's reason and the diff of what it changes. */
function OwnChange({ item }: { item: InboxItem }) {
  const reason = typeof item.data.reason === "string" ? item.data.reason : "";
  const diff = typeof item.data.diff === "string" ? item.data.diff : "";
  return (
    <div className="flex flex-col gap-3">
      {reason && (
        <p className="m-0 max-w-prose text-base text-secondary [overflow-wrap:anywhere]">
          {reason}
        </p>
      )}
      <CodeBlock language="diff" code={diff}>
        {diff}
      </CodeBlock>
    </div>
  );
}

/** What the trials of a proposal found, one line each; then, under a label, the same for the regression set. */
function Trials({ item }: { item: InboxItem }) {
  const line = (trial: ReturnType<typeof trialsOf>[number], index: number) => (
    <li key={`${trial.card}-${index}`} className="flex flex-wrap items-center gap-3">
      <Tag>{trial.card}</Tag>
      <Status tone={trial.positive ? "complete" : "neutral"}>{trial.outcome}</Status>
      <span className="min-w-0 flex-1 break-words text-base text-secondary">{trial.reason}</span>
    </li>
  );
  const regression = trialsOf(item, "regression");
  return (
    <div className="flex flex-col gap-3">
      {/* biome-ignore lint/a11y/noRedundantRoles: preflight sets list-style: none, and WebKit then drops a list with
          no marker from the accessibility tree entirely */}
      <ul role="list" className="m-0 flex list-none flex-col gap-2 p-0">
        {trialsOf(item, "trials").map(line)}
      </ul>
      {regression.length > 0 && (
        <>
          <p className="m-0 text-sm text-secondary">
            Regression set: past merged cards it must not break
          </p>
          {/* biome-ignore lint/a11y/noRedundantRoles: preflight sets list-style: none, and WebKit then drops a list
              with no marker from the accessibility tree entirely */}
          <ul role="list" className="m-0 flex list-none flex-col gap-2 p-0">
            {regression.map(line)}
          </ul>
        </>
      )}
    </div>
  );
}

/** The issues a grilling proposes as a list to edit: a title field and a body for each, with what blocks it. */
function Issues({ drafts, onChange }: { drafts: Draft[]; onChange: (drafts: Draft[]) => void }) {
  if (drafts.length === 0) {
    return (
      <p className="m-0 text-base text-secondary">
        No issue is left. Dismiss the item, or ask the agent again.
      </p>
    );
  }
  const edit = (index: number, change: Partial<Draft>) =>
    onChange(drafts.map((draft, at) => (at === index ? { ...draft, ...change } : draft)));
  return (
    <>
      {/* biome-ignore lint/a11y/noRedundantRoles: preflight sets list-style: none, and WebKit then drops a list with
          no marker from the accessibility tree entirely */}
      <ol role="list" className="m-0 flex list-none flex-col gap-3 p-0">
        {drafts.map((draft, index) => (
          // The list is edited in place and issues are removed by position, so the position is the key.
          // biome-ignore lint/suspicious/noArrayIndexKey: see above
          <li key={index} className="flex flex-col gap-3 rounded-control bg-sunken p-4">
            <div className="flex flex-wrap items-baseline justify-between gap-3">
              {draft.blockedBy.length > 0 && (
                <span className="text-sm text-secondary">
                  Blocked by issue {draft.blockedBy.map((blocker) => blocker + 1).join(", ")}
                </span>
              )}
              <Button
                size="sm"
                variant="danger"
                className="ml-auto"
                aria-label={`Remove issue ${index + 1}`}
                onClick={() => onChange(removeIssue(drafts, index))}
              >
                Remove
              </Button>
            </div>
            <Field
              label={`Title of issue ${index + 1}`}
              // The empty title is what greys out "Confirm", so the reason sits on the field it belongs to.
              error={draft.title.trim() === "" ? "Write a title to confirm this issue." : undefined}
            >
              {(id) => (
                <Input
                  id={id}
                  value={draft.title}
                  onChange={(e) => edit(index, { title: e.target.value })}
                />
              )}
            </Field>
            <Field label={`Description of issue ${index + 1}`} hint="What the issue asks for.">
              {(id) => (
                <TextArea
                  id={id}
                  value={draft.body}
                  rows={3}
                  onChange={(e) => edit(index, { body: e.target.value })}
                />
              )}
            </Field>
          </li>
        ))}
      </ol>
    </>
  );
}
