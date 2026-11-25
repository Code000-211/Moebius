/**
 * What to decide about the item chosen in the Inbox, drawn in the window the Inbox opens for it: why it is here in a word and a
 * sentence, what it shows (the facts of a change, the change a proposal offers with the trials it was put through, the
 * issues of a grilling to edit, a report) and the buttons that act on it (merge, approve, send back). While a request
 * is on its way the buttons give way to what it does; a refusal of the server stays under them in the server's own
 * words. The item leaves the list when the snapshot is read again, which the answer and the event cause.
 */

import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { useEffect, useState } from "react";
import { Link, useNavigate } from "react-router";
import { proposals } from "../../api/calls/improve";
import { isApiError } from "../../api/http";
import { keys } from "../../api/keys";
import type { InboxItem, Repository, Snapshot } from "../../api/types/board";
import type { Proposal } from "../../api/types/improve";
import { CodeBlock } from "../../ui/chat/CodeBlock";
import { Markdown } from "../../ui/chat/Markdown";
import { Button } from "../../ui/components/Button";
import { Input } from "../../ui/components/Input";
import { PrBadge } from "../../ui/components/PrBadge";
import { cn } from "../../ui/lib/cn";
import { ago } from "../../ui/lib/format";
import { useNow } from "../../ui/lib/useNow";
import { Field, TextArea } from "../../ui/page/Field";
import { Notice } from "../../ui/page/Notice";
import { SkeletonCard } from "../../ui/page/Skeleton";
import { useOpenCard } from "../board/CardWindowHost";
import { pullRequestOf } from "../board/model";
import { repositoryBoardPath } from "../pages";
import { useRole } from "../shell/session";
import { type Action, perform, working } from "./actions";
import { DeleteGrilling } from "./DeleteGrilling";
import { DigestView, digestOf } from "./DigestView";
import {
  APPROVE_ISSUE,
  CHANGE_CLOSED,
  changeOf,
  chatOf,
  confirmable,
  DAILY_DIGEST,
  DONE_ASKED,
  type Draft,
  doneLogin,
  draftsOf,
  type Fact,
  facts,
  found,
  GRILLING_ISSUES,
  GRILLING_REPORT,
  grillingOf,
  issuesSent,
  kindTag,
  MERGE_REFUSED,
  MERGE_STOPPED,
  meta,
  PROPOSAL,
  proposalOf,
  proposedOf,
  READY_TO_MERGE,
  REVIEW_NOT_POSSIBLE,
  REVIEW_ROUNDS_SPENT,
  removeIssue,
  STUCK,
  TAKEOVER,
  takeoverOf,
  trialsOf,
  WIKI_PROPOSAL,
  why,
  wikiProposalOf,
} from "./model";

/** The proposals the Retro stored, read once and shared by the window's bar and what it shows. */
function useProposal(item: InboxItem) {
  const proposal = proposalOf(item);
  const record = useQuery({
    queryKey: keys.proposals,
    queryFn: proposals,
    enabled: item.kind === PROPOSAL && proposal !== null,
  });
  return {
    pending: record.isPending && item.kind === PROPOSAL && proposal !== null,
    proposed: record.data?.find((each) => each.id === proposal) ?? null,
  };
}

/** The file a proposal changes: the one its diff names, or the agent's file for a change to instructions. */
export function fileOf(item: InboxItem, proposed: Proposal | null): string | null {
  const diff = proposedOf(item, proposed).diff;
  const named = /^\+\+\+ (?:b\/)?(\S+)/m.exec(diff)?.[1];
  if (named && named !== "/dev/null") return named;
  const agent = proposed?.agents?.[0];
  return agent ? `agents/${agent}.md` : null;
}

/**
 * What the Decide window's top bar says: the item's kind and, where it has one, the file it concerns, and the page
 * the item opens as (the Retro's day for a proposal, the card, the chat) for "Open as page".
 */
export function useDecisionBar(item: InboxItem, snapshot: Snapshot) {
  const { proposed } = useProposal(item);
  const { label } = kindTag(item);
  const file = item.kind === PROPOSAL ? fileOf(item, proposed) : null;
  return {
    leading: (
      <>
        <span className="shrink-0 font-medium text-text">{label}</span>
        {file && (
          <>
            <span aria-hidden="true">·</span>
            <span className="truncate font-mono">{file}</span>
          </>
        )}
      </>
    ),
    href: linkOf(item, found(snapshot, item).board?.repository ?? null, proposed),
  };
}

export function Decision({
  item,
  snapshot,
  onDirty,
}: {
  item: InboxItem;
  snapshot: Snapshot;
  /** Told whether the person has typed something that is not sent yet (an edited issue, a reason), so the window guards it. */
  onDirty?: (dirty: boolean) => void;
}) {
  const queryClient = useQueryClient();
  const navigate = useNavigate();
  const openCard = useOpenCard();
  const about = found(snapshot, item);
  const now = useNow();
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
  const unsent =
    (item.kind === GRILLING_ISSUES && JSON.stringify(drafts) !== JSON.stringify(draftsOf(item))) ||
    (rejecting !== null && rejecting.trim() !== "");
  // biome-ignore lint/correctness/useExhaustiveDependencies: told when it changes; the callback is the page's setter
  useEffect(() => {
    onDirty?.(unsent);
    return () => onDirty?.(false);
  }, [unsent]);
  const act = (action: Action) => {
    if (mutation.isPending) return;
    setNotice(null);
    mutation.mutate(action);
  };

  // A viewer reads the proposed change; a member decides it.
  const role = useRole();
  const change = changeOf(item);
  const grilling = grillingOf(item);
  const proposal = proposalOf(item);
  const takeover = takeoverOf(item);
  // What a proposal offers lives on the proposal rather than on the item: the one made when the Retro stores it
  // carries the change itself, and the one a trial ends with names its proposal and nothing else. The card reads the
  // list once and looks for the one it is about, which is the thing it is asked to show.
  const { proposed, pending: reading } = useProposal(item);
  const sentence = why(item, about, proposed?.kind);
  const href = linkOf(item, about.board?.repository ?? null, proposed);
  const from = item.kind === PROPOSAL ? fromRetro(item, proposed, now) : meta(item, about);
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
          plain("Open live view", board ? () => openCard(item.card_id as number) : null),
        );
        buttons.push(plain("Reject with a reason", () => setRejecting("")));
      } else {
        const chat = chatOf(item);
        buttons.push(plain("Open chat", chat === null ? null : () => navigate(`/chat/${chat}`)));
      }
    } else if (
      (item.kind === MERGE_STOPPED || item.kind === MERGE_REFUSED) &&
      item.card_id !== null
    ) {
      // What stopped is the forge's or a person's to fix; the card goes on when its last change is merged.
      const board = about.board;
      buttons.push(plain("Open the card", board ? () => openCard(item.card_id as number) : null));
    } else if (
      (item.kind === REVIEW_ROUNDS_SPENT || item.kind === REVIEW_NOT_POSSIBLE) &&
      item.card_id !== null
    ) {
      // Its choices (review again, merge) are the card's, as for the pull request itself.
      const board = about.board;
      buttons.push(plain("Open the card", board ? () => openCard(item.card_id as number) : null));
    } else if (item.kind === APPROVE_ISSUE && item.card_id !== null) {
      buttons.push(primary("Approve", { kind: "approve", card: item.card_id }));
    } else if (item.kind === DONE_ASKED && item.card_id !== null) {
      buttons.push(
        primary("Close the issue", { kind: "done", card: item.card_id, answer: "close" }),
      );
      buttons.push(
        plain(`Give it back to @${doneLogin(item)}`, () =>
          act({ kind: "done", card: item.card_id as number, answer: "give_back" }),
        ),
      );
      buttons.push(
        plain("Release it", () =>
          act({ kind: "done", card: item.card_id as number, answer: "release" }),
        ),
      );
    } else if ((item.kind === STUCK || item.kind === CHANGE_CLOSED) && item.card_id !== null) {
      buttons.push(primary("Retry", { kind: "retry", card: item.card_id }));
    } else if (item.kind === PROPOSAL && proposal !== null) {
      buttons.push(plain("Dismiss", () => act({ kind: "dismissProposal", proposal })));
      buttons.push(primary("Merge", { kind: "mergeProposal", proposal }));
    } else if (item.kind === TAKEOVER && takeover !== null) {
      buttons.push(primary("Merge", { kind: "mergeTakeover", takeover }));
      buttons.push(plain("Dismiss", () => act({ kind: "dismissTakeover", takeover })));
    } else if (item.kind === WIKI_PROPOSAL && role !== "viewer") {
      buttons.push(primary("Merge", { kind: "mergeWikiProposal", item: item.id }));
      buttons.push(plain("Dismiss", () => act({ kind: "dismissWikiProposal", item: item.id })));
    } else if (item.kind === GRILLING_ISSUES && grilling !== null) {
      buttons.push(
        primary(
          "Confirm",
          { kind: "confirmGrilling", grilling, issues: issuesSent(drafts) },
          !confirmable(drafts),
        ),
      );
      buttons.push(plain("Dismiss", () => act({ kind: "dismissGrilling", grilling })));
      buttons.push(<DeleteGrilling key="delete" grilling={grilling} />);
    } else if (item.kind === DAILY_DIGEST) {
      buttons.push(primary("Dismiss", { kind: "dismissDigest", item: item.id }));
    } else if (item.kind === GRILLING_REPORT && grilling !== null) {
      buttons.push(primary("Dismiss", { kind: "dismissGrilling", grilling }));
      buttons.push(<DeleteGrilling key="delete" grilling={grilling} />);
    }
    return <div className="flex flex-wrap justify-end gap-2">{buttons}</div>;
  };

  return (
    <div data-testid="inbox-decision" data-kind={item.kind} className="flex flex-col gap-3">
      <div className="flex flex-col gap-2">
        <h2 className="m-0 break-words text-base font-semibold tracking-tight text-text">
          {href ? (
            <Link
              to={href}
              className="text-inherit no-underline transition-colors duration-(--motion-routine) ease-out hover:underline"
            >
              {item.title}
            </Link>
          ) : (
            item.title
          )}
        </h2>
        {from !== "" && <p className="m-0 text-sm text-secondary">{from}</p>}
        {sentence && (
          <p className="m-0 max-w-prose text-base text-secondary [overflow-wrap:anywhere]">
            {sentence}
          </p>
        )}
      </div>

      {item.kind === READY_TO_MERGE && <Facts item={item} snapshot={snapshot} />}
      {item.kind === PROPOSAL &&
        (reading && proposedOf(item, null).reason === "" ? (
          <SkeletonCard lines={2} message="Reading the proposal" />
        ) : (
          <OwnChange item={item} proposed={proposed} />
        ))}
      {item.kind === PROPOSAL && <Trials item={item} />}
      {item.kind === WIKI_PROPOSAL && wikiProposalOf(item).diff !== "" && (
        <CodeBlock language="diff" code={wikiProposalOf(item).diff} />
      )}
      {item.kind === DAILY_DIGEST && digestOf(item) !== null && (
        <DigestView digest={digestOf(item) as NonNullable<ReturnType<typeof digestOf>>} />
      )}
      {item.kind === GRILLING_ISSUES && <Issues drafts={drafts} onChange={setDrafts} />}
      {item.kind === GRILLING_REPORT && (
        <div className="rounded-control bg-sunken p-4">
          <Markdown
            content={typeof item.data.report === "string" ? item.data.report : ""}
            keepNewlines
            className="max-w-prose"
          />
        </div>
      )}
      {href !== null && item.card_id !== null && (
        <Link
          to={href}
          className="w-fit text-sm text-accent no-underline hover:underline"
          data-testid="open-card"
        >
          Open card
        </Link>
      )}
      {footer()}
      {notice && <Notice tone="error">{notice}</Notice>}
    </div>
  );
}

function Facts({ item, snapshot }: { item: InboxItem; snapshot: Snapshot }) {
  const about = found(snapshot, item);
  const pullRequest = about.board
    ? pullRequestOf(about.changeRepository ?? about.board.repository, about.change)
    : null;
  return (
    <>
      {pullRequest && (
        <PrBadge
          number={pullRequest.number}
          url={pullRequest.url}
          status={pullRequest.status}
          className="w-fit"
        />
      )}
      <FactList facts={facts(item, about)} />
    </>
  );
}

function FactList({ facts }: { facts: Fact[] }) {
  return (
    <dl className="m-0 flex flex-wrap gap-2">
      {facts.map((fact) => (
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

/**
 * Where the item itself is opened, so its title is a link: the card on its board, the chat a change came from, the
 * Retro's day for a proposal, the board of a grilling's repository. A takeover has no page of its own.
 */
function linkOf(
  item: InboxItem,
  board: Repository | null,
  proposed: Proposal | null,
): string | null {
  if (item.card_id !== null && board !== null) return repositoryBoardPath(board, item.card_id);
  const chat = chatOf(item);
  if (chat !== null) return `/chat/${chat}`;
  if (proposed) return retroLink(proposed);
  if (grillingOf(item) !== null && board !== null) return repositoryBoardPath(board);
  return null;
}

/** Where a proposal comes from: the Retro of its day and how long ago the item was made. */
function fromRetro(item: InboxItem, proposed: Proposal | null, now: number): string {
  const day = proposed?.retro_date;
  const age = item.at > 0 ? ago(now, item.at) : "";
  return [day ? `From the Retro of ${day}` : "From the Retro", age].filter(Boolean).join(" · ");
}

/** Where a proposal is read in full: the day of the Retro that made it, opened on the proposal itself. */
function retroLink(proposed: Proposal): string {
  const day = encodeURIComponent(proposed.retro_date);
  return `/improve?day=${day}&proposal=${encodeURIComponent(String(proposed.id))}`;
}

/**
 * What a proposal would change: the Retro's reason for it, three lines at most, and the diff when the change comes
 * as one. The whole of a proposal is read on the Retro page rather than on a card that lists them, so the reason
 * stops at what tells one from the next and is itself the way there.
 */
function OwnChange({ item, proposed }: { item: InboxItem; proposed: Proposal | null }) {
  const navigate = useNavigate();
  const shown = proposedOf(item, proposed);
  const open = () => {
    if (proposed) navigate(retroLink(proposed));
  };
  if (shown.reason === "" && shown.diff === "") return null;
  return (
    <div className="flex flex-col gap-3">
      {shown.reason !== "" && (
        // Three lines at most: the whole of the reason is a click away on the Retro page, so a
        // card that lists proposals does not have to hold all of it. With no proposal to open it stays plain text.
        <button
          type="button"
          disabled={proposed === null}
          onClick={open}
          className="flex min-h-8 w-full max-w-prose cursor-pointer items-center text-left text-base text-secondary transition-colors duration-(--motion-routine) ease-out hover:text-text disabled:cursor-default disabled:hover:text-secondary"
        >
          <span className="clamp-3 break-words">{shown.reason}</span>
        </button>
      )}
      {shown.diff !== "" && <CodeBlock language="diff" code={shown.diff} />}
    </div>
  );
}

/**
 * What the trials of a proposal found, one line each; then, under a label, the same for the regression set. A trial
 * that names no card only says there was nothing to try the proposal on, which is not something found.
 */
function Trials({ item }: { item: InboxItem }) {
  const line = (trial: ReturnType<typeof trialsOf>[number], index: number) => (
    <li key={`${trial.card}-${index}`} className="flex flex-wrap items-baseline gap-3">
      <span className="shrink-0 text-sm text-secondary tabular-nums">{`Card ${trial.card}`}</span>
      <span
        className={cn(
          "shrink-0 text-sm font-semibold",
          trial.tone === "complete"
            ? "text-complete-text"
            : trial.tone === "error"
              ? "text-error-text"
              : "text-secondary",
        )}
      >
        {trial.outcome}
      </span>
      <span className="min-w-0 flex-1 break-words text-base text-secondary">{trial.reason}</span>
    </li>
  );
  const tried = trialsOf(item, "trials").filter((trial) => trial.card !== null);
  const regression = trialsOf(item, "regression").filter((trial) => trial.card !== null);
  if (tried.length === 0 && regression.length === 0) return null;
  return (
    <div className="flex flex-col gap-3">
      {/* biome-ignore lint/a11y/noRedundantRoles: preflight sets list-style: none, and WebKit then drops a list with
          no marker from the accessibility tree entirely */}
      <ul role="list" className="m-0 flex list-none flex-col gap-2 p-0">
        {tried.map(line)}
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
