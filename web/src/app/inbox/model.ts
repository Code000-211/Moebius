/**
 * The logic of the Inbox page, apart from React: what an item is about (looked up in the snapshot), its kind as a tag
 * and a sentence, the facts of a change that waits, the filters and their counts, and the list of issues a grilling
 * proposes while a person edits it (the former desktop app's `screens/inbox`).
 */
import type { Board, Card, Change, InboxItem, Repository, Snapshot } from "../../api/types/board";
import type { Proposal } from "../../api/types/improve";
import type { GrillingIssue } from "../../api/types/inbox";
import type { StateMark } from "../../ui/page/MarkLabel";
import type { StatusState } from "../../ui/page/StatusIcon";
import type { ListItem } from "../../ui/page/VirtualList";
import { mergeStopped, tokens } from "../board/model";

export const READY_TO_MERGE = "ready_to_merge";
export const APPROVE_ISSUE = "approve_issue";
export const STUCK = "stuck";
export const CHANGE_CLOSED = "change_closed";
export const MERGE_STOPPED = "merge_stopped";
export const PROPOSAL = "proposal";
export const TAKEOVER = "takeover";
export const WIKI_PROPOSAL = "wiki_proposal";
export const GRILLING_ISSUES = "grilling_issues";
export const GRILLING_REPORT = "grilling_report";

/** The branch of a change that offers the work of a chat is `moebius/chat-<id>`. */
const CHAT_BRANCH = "moebius/chat-";

const number = (value: unknown): number | null => (typeof value === "number" ? value : null);
const text = (value: unknown): string => (typeof value === "string" ? value : "");

/** The id of the change a `ready_to_merge` or `change_closed` item is about. */
export const changeOf = (item: InboxItem) => number(item.data.change);
export const proposalOf = (item: InboxItem) => number(item.data.proposal);
/** What a `wiki_proposal` item offers: the page it changes and the diff of the change (the server cuts it at 40 000 bytes). */
export const wikiProposalOf = (item: InboxItem) => ({
  page: text(item.data.page),
  by: text(item.data.by),
  diff: text(item.data.diff),
});
export const takeoverOf = (item: InboxItem) => number(item.data.takeover);
export const grillingOf = (item: InboxItem) => number(item.data.grilling);
/** The id of the repository a grilling's or a proposal's item is about. */
export const repositoryOf = (item: InboxItem) => number(item.data.repository);

/** The id of the chat whose work a `ready_to_merge` item offers: the number in its branch. */
export function chatOf(item: InboxItem): number | null {
  const branch = text(item.data.branch);
  if (!branch.startsWith(CHAT_BRANCH)) return null;
  const id = branch.slice(CHAT_BRANCH.length);
  return /^\d+$/.test(id) ? Number(id) : null;
}

/** The issues a `grilling_issues` item proposes. */
export function issuesOf(item: InboxItem): GrillingIssue[] {
  const issues = Array.isArray(item.data.issues) ? item.data.issues : [];
  return issues.map((each) => {
    const issue = (typeof each === "object" && each !== null ? each : {}) as Record<
      string,
      unknown
    >;
    return {
      title: text(issue.title),
      body: text(issue.body),
      blocked_by: Array.isArray(issue.blocked_by)
        ? issue.blocked_by.filter((n): n is number => typeof n === "number")
        : [],
    };
  });
}

/** What an item is about, looked up in the snapshot: the board of its repository, its card and its change. */
export interface Found {
  board?: Board;
  card?: Card;
  change?: Change;
  /** The repository the change is in, which is not the card's when the card works in several. */
  changeRepository?: Repository;
}

export function found(snapshot: Snapshot, item: InboxItem): Found {
  const changeId = changeOf(item);
  // A card's change in another repository is on the board of that repository, so the card's own board comes first.
  const board =
    snapshot.boards.find((each) => each.cards.some((card) => card.id === item.card_id)) ??
    snapshot.boards.find(
      (each) =>
        each.changes.some((change) => change.id === changeId) ||
        ((grillingOf(item) !== null || item.kind === PROPOSAL) &&
          repositoryOf(item) === each.repository.id),
    );
  // The change is on the board of its repository; failing that, the card says which repository it is in.
  const card = board?.cards.find((each) => each.id === item.card_id);
  const named = card?.changes?.find((change) => change.id === changeId)?.repository;
  const changeBoard =
    snapshot.boards.find((each) => each.changes.some((change) => change.id === changeId)) ??
    snapshot.boards.find((each) => named !== undefined && each.repository.name === named);
  return {
    board,
    card,
    change:
      changeBoard?.changes.find((change) => change.id === changeId) ??
      snapshot.harness_changes.find((change) => change.id === changeId),
    changeRepository: changeBoard?.repository,
  };
}

/** A kind of a column script, such as `needs_design`, as a label: `Needs design`. */
export function sentence(kind: string): string {
  const words = kind.replaceAll("_", " ");
  return words.charAt(0).toUpperCase() + words.slice(1);
}

/**
 * What the item is, said in a word, with the blocked icon before what is stuck and the ready-to-merge icon before a
 * change that waits for the person's merge; every other kind is the word alone, as grey text.
 */
export function kindTag(item: InboxItem): { label: string; mark: StateMark } {
  switch (item.kind) {
    case STUCK:
      return { label: "Stuck", mark: "blocked" };
    case READY_TO_MERGE:
      return item.card_id === null
        ? { label: "Change from a chat", mark: "none" }
        : { label: "Ready to merge", mark: "readyToMerge" };
    case APPROVE_ISSUE:
      return { label: "Outside issue", mark: "none" };
    case CHANGE_CLOSED:
      return { label: "Closed without a merge", mark: "none" };
    case MERGE_STOPPED:
      return { label: "Merge stopped", mark: "blocked" };
    case PROPOSAL:
      return { label: "Retro proposal", mark: "none" };
    case TAKEOVER:
      return { label: "Decision model", mark: "none" };
    case WIKI_PROPOSAL:
      return { label: "Wiki edit", mark: "none" };
    case GRILLING_ISSUES:
      return { label: "Issues from a grilling", mark: "none" };
    case GRILLING_REPORT:
      return { label: "Report from a grilling", mark: "none" };
    default:
      return { label: sentence(item.kind), mark: "none" };
  }
}

/**
 * The sentence under the title: why the item is here. `changes` is what a proposal changes, for the items that do not
 * say so themselves: the one a trial ends with names its proposal and nothing else.
 */
export function why(item: InboxItem, about: Found, changes?: string): string | null {
  const reason = text(item.data.reason);
  switch (item.kind) {
    case STUCK:
      return reason ? `Stopped: ${reason}.` : "The card is stuck.";
    case APPROVE_ISSUE:
      return reason
        ? `A person has to approve this issue: ${reason}.`
        : "A person has to approve this issue.";
    case CHANGE_CLOSED: {
      const pull = number(item.data.pull_request) ?? about.change?.pull_request ?? null;
      return pull !== null
        ? `Pull request #${pull} was closed without a merge.`
        : "The change's pull request was closed without a merge.";
    }
    case READY_TO_MERGE:
      return null;
    case MERGE_STOPPED: {
      const { repository, reason, merged, open } = mergeStopped(item);
      const stopped = `The merge in ${repository || "a repository"} stopped${reason !== "" ? `: ${reason}` : ""}.`;
      const done = merged.length > 0 ? ` Merged: ${merged.join(", ")}.` : "";
      return `${stopped}${done} Still open: ${open.join(", ")}.`;
    }
    case TAKEOVER: {
      const agreement = Math.round(
        (typeof item.data.agreement === "number" ? item.data.agreement : 0) * 100,
      );
      const count = number(item.data.count) ?? 0;
      return `It chose what was recorded on ${agreement}% of ${count} judgements held back from training. Merging lets it decide this kind from now on.`;
    }
    case WIKI_PROPOSAL: {
      const { page, by } = wikiProposalOf(item);
      const who = by !== "" ? `${by} proposes` : "A change is proposed for";
      return `${who} this edit of ${page !== "" ? `the page ${page}` : "a page"}. Merging changes the wiki; dismissing leaves it as it is.`;
    }
    case PROPOSAL: {
      const own = text(item.data.kind) || (changes ?? "");
      if (own === "settings") {
        return "The Retro proposes this change to moebius.toml. Merging saves it, and restarts the server when a key needs it.";
      }
      if (own === "repository") {
        return "The Retro proposes this change to the repository's own rules or column scripts. Merging offers it as a change on the repository.";
      }
      return "The Retro proposes this change to the harness configuration.";
    }
    case GRILLING_ISSUES:
      return "The Grilling partner ended in these issues. Edit them, remove what you do not want, and confirm to make a card of each.";
    case GRILLING_REPORT:
      return "The Grilling partner ended in a research report. Read it, then dismiss it.";
    default:
      return reason || null;
  }
}

/**
 * What the words above the title say: the repository, the card's number and who wrote it, or what the item comes
 * from. A proposal says that already in its own kind, so it adds nothing here.
 */
export function meta(item: InboxItem, about: Found): string {
  const parts: string[] = [];
  // A change of a card over several repositories is named by the repository it is in, not the card's.
  const repository = (about.changeRepository ?? about.board?.repository)?.name;
  if (repository !== undefined) parts.push(repository);
  if (about.card) {
    parts.push(`#${about.card.number}`);
    if (item.kind === APPROVE_ISSUE && about.card.author !== "")
      parts.push(`by ${about.card.author}`);
  } else if (grillingOf(item) !== null) {
    parts.push("Grilling");
  } else if (item.card_id === null && item.kind !== PROPOSAL) {
    parts.push("chat");
  }
  if (about.change?.harness_repository) parts.unshift("Harness configuration");
  return parts.join(" · ");
}

export interface Fact {
  label: string;
  value: string;
  /** Teal for a check that went through. */
  good?: boolean;
  mono?: boolean;
}

/** The facts of a change that waits to be merged: what it took, how its checks stand and where it is. */
export function facts(item: InboxItem, about: Found): Fact[] {
  const list: Fact[] = [];
  if (about.card) {
    list.push({ label: "Rounds", value: String(about.card.rounds) });
    list.push({ label: "Tokens", value: tokens(about.card.tokens) });
  }
  const checks = about.change?.checks;
  const label =
    checks === "success"
      ? "CI green"
      : checks === "pending"
        ? "CI running"
        : checks === "failure"
          ? "CI failed"
          : "No checks";
  const review = about.change?.review;
  list.push({
    label: "Checks",
    value: review ? `${label} · review ${review}` : label,
    good: checks === "success",
  });
  if (about.card && (about.card.repositories?.length ?? 1) > 1 && about.changeRepository) {
    list.push({ label: "Repository", value: about.changeRepository.name });
  }
  const branch = text(item.data.branch) || about.change?.branch;
  if (branch) list.push({ label: "Branch", value: branch, mono: true });
  return list;
}

export interface Trial {
  /** The card the trial ran on, or `null` when the proposal names none — there is then nothing to try it on. */
  card: number | null;
  outcome: string;
  /** How the outcome reads in words alone: teal for what worked, red for what did not, quiet for the rest. */
  tone: "complete" | "error" | "quiet";
  reason: string;
}

/**
 * What the card of a proposal says about the change itself: the Retro's reason for it and the diff, for the changes
 * that come as one. An item made when the Retro stores a proposal carries both; the one a trial ends with carries
 * neither, so the proposal read back from the server stands in.
 */
export function proposedOf(
  item: InboxItem,
  proposed: Proposal | null,
): { reason: string; diff: string } {
  const own = text(item.data.reason);
  const diff = typeof item.data.diff === "string" ? text(item.data.diff) : "";
  return {
    reason: own !== "" ? own : (proposed?.body ?? ""),
    diff: diff !== "" ? diff : (proposed?.diff ?? ""),
  };
}

/** The trials of a proposal (`key` is `trials`) or of its regression set (`regression`), one line each. */
export function trialsOf(item: InboxItem, key: "trials" | "regression"): Trial[] {
  const list = Array.isArray(item.data[key]) ? (item.data[key] as unknown[]) : [];
  return list.map((each) => {
    const trial = (typeof each === "object" && each !== null ? each : {}) as Record<
      string,
      unknown
    >;
    const card = number(trial.card);
    const outcome = text(trial.outcome);
    return {
      card,
      outcome: sentence(outcome),
      tone:
        outcome === "positive"
          ? "complete"
          : outcome === "negative" || outcome === "failed"
            ? "error"
            : "quiet",
      reason: text(trial.reason),
    };
  });
}

export interface Filters {
  kind: string | null;
  repository: number | null;
  /** Only the items about this change: where a link from the package sheet lands. */
  change?: number | null;
}

export const NO_FILTERS: Filters = { kind: null, repository: null, change: null };

const inChange = (filters: Filters, item: InboxItem) =>
  filters.change == null || changeOf(item) === filters.change;

const inKind = (filters: Filters, item: InboxItem) =>
  filters.kind === null || filters.kind === item.kind;

const inRepository = (snapshot: Snapshot, repository: number | null, item: InboxItem) =>
  repository === null || found(snapshot, item).board?.repository.id === repository;

/** The items the filters let through, newest first. */
export function shown(snapshot: Snapshot, filters: Filters): InboxItem[] {
  return snapshot.inbox
    .filter(
      (item) =>
        inKind(filters, item) &&
        inChange(filters, item) &&
        inRepository(snapshot, filters.repository, item),
    )
    .sort((a, b) => b.at - a.at || b.id - a.id);
}

/** One issue of a grilling as the person edits it. */
export interface Draft {
  title: string;
  body: string;
  blockedBy: number[];
}

export function draftsOf(item: InboxItem): Draft[] {
  return issuesOf(item).map((issue) => ({
    title: issue.title,
    body: issue.body,
    blockedBy: issue.blocked_by,
  }));
}

/** The list without the issue at `index`: the issues after it move up, and nothing waits for it any more. */
export function removeIssue(drafts: Draft[], index: number): Draft[] {
  if (index < 0 || index >= drafts.length) return drafts;
  return drafts
    .filter((_, at) => at !== index)
    .map((draft) => ({
      ...draft,
      blockedBy: draft.blockedBy
        .filter((blocker) => blocker !== index)
        .map((blocker) => (blocker > index ? blocker - 1 : blocker)),
    }));
}

/** The list as it is sent: trimmed titles and bodies. */
export function issuesSent(drafts: Draft[]): GrillingIssue[] {
  return drafts.map((draft) => ({
    title: draft.title.trim(),
    body: draft.body.trim(),
    blocked_by: draft.blockedBy,
  }));
}

/** Whether the list can be confirmed: it has an issue, and each has a title. */
export function confirmable(drafts: Draft[]): boolean {
  return drafts.length > 0 && drafts.every((draft) => draft.title.trim() !== "");
}

/** What a row of the list is about: an item of the inbox, or a card that is running or is done. */
export interface Entry {
  /** Names the row in the list and in the address: `item-<id>` or `card-<id>`. */
  key: string;
  status: StatusState;
  /** The key of what it is about, drawn in mono. */
  id: string;
  title: string;
  /** The one pale reason or activity. */
  reason: string;
  /** The handle of the person it is given to; none for a run of an agent. */
  owner?: string;
  /** Unix seconds, or null when the snapshot does not say. */
  at: number | null;
  /** The inbox item; null for a card. */
  item: InboxItem | null;
  /** The card a click on the row opens in the card window; null for an item that is about none. */
  card: number | null;
  /** The card the one action of the row, Approve plan, approves; null when the row needs no approval. */
  approves: number | null;
}

/** The key of a card as a person says it: `WEB-24` with a project key, else `#12`. */
export function cardKey(card: Card): string {
  return card.project_key && card.project_number
    ? `${card.project_key}-${card.project_number}`
    : `#${card.number}`;
}

/** An item that is an outsider's issue waiting for approval is a plan ready: the one thing the row can approve. */
export const approvable = (item: InboxItem): boolean =>
  item.kind === APPROVE_ISSUE && item.card_id !== null;

/**
 * The title of an item as the row draws it. The server words some titles with their reason ("… is ready to merge",
 * "Approve: …"); the reason belongs to the pale text only, so the row's title is the card's. A card in the snapshot gives its own
 * title, whatever sentence the server worded the item with.
 */
export function titleOf(item: InboxItem): string {
  const plain = item.title.replace(/^Approve: /, "").replace(/ is ready to merge$/, "");
  return plain === "" ? item.title : plain;
}

/** The one pale reason a row of the inbox gives: "Plan ready", or the kind's word with why it stopped. */
export function rowReason(item: InboxItem): string {
  if (approvable(item)) return "Plan ready";
  const { label } = kindTag(item);
  const reason = text(item.data.reason);
  return reason !== "" && (item.kind === STUCK || item.kind === MERGE_STOPPED)
    ? `${label}: ${reason}`
    : label;
}

/** How many closed cards the Earlier group shows. */
export const EARLIER = 10;

const working = (card: Card) => card.run?.status === "starting" || card.run?.status === "running";

function activity(card: Card): string {
  const purpose = card.run?.purpose ?? "";
  const word =
    purpose === "code"
      ? "Coding"
      : purpose === "review"
        ? "Reviewing"
        : sentence(purpose || "running");
  return card.rounds > 1 ? `${word}, round ${card.rounds}` : word;
}

/**
 * The list of the page: what needs the person, newest first; the cards an agent is working on; and the last cards that
 * are done. A card that already has an item in the first group is not repeated in the others. Under a repository or a
 * change filter only the first group is drawn, because the other two are about cards and say nothing of a change.
 */
export function listOf(
  snapshot: Snapshot,
  items: InboxItem[],
  filters: Filters,
): ListItem<Entry>[] {
  const out: ListItem<Entry>[] = [];
  const listed = new Set<number>();
  if (items.length > 0) out.push({ type: "group", key: "group-needs-you", name: "Needs you" });
  for (const item of items) {
    const about = found(snapshot, item);
    if (item.card_id !== null) listed.add(item.card_id);
    // Only a card has a key to lead with; the kind's word under it already says a proposal or a change.
    const key = about.card ? cardKey(about.card) : "";
    out.push({
      type: "row",
      key: `item-${item.id}`,
      data: {
        key: `item-${item.id}`,
        status: item.kind === READY_TO_MERGE ? "readyToMerge" : "needsYou",
        id: key,
        title: about.card?.title ?? titleOf(item),
        reason: rowReason(item),
        owner: item.assignees[0],
        at: item.at,
        item,
        card: item.card_id !== null && about.board ? item.card_id : null,
        approves: approvable(item) ? item.card_id : null,
      },
    });
  }
  if (filters.change != null || filters.kind !== null) return out;
  const boards = snapshot.boards.filter(
    (board) => filters.repository === null || board.repository.id === filters.repository,
  );
  const cards = boards.flatMap((board) => board.cards).filter((card) => !listed.has(card.id));
  const entry = (card: Card, status: StatusState, reason: string): ListItem<Entry> => ({
    type: "row",
    key: `card-${card.id}`,
    data: {
      key: `card-${card.id}`,
      status,
      id: cardKey(card),
      title: card.title,
      reason,
      owner: card.assignees[0],
      // A card in Earlier says how long ago it was made, the only age the snapshot gives it; one at work has none.
      at: status === "done" ? card.created_at : null,
      item: null,
      card: card.id,
      approves: null,
    },
  });
  const running = cards.filter(working);
  if (running.length > 0) {
    out.push({ type: "group", key: "group-running", name: "Running" });
    for (const card of running) out.push(entry(card, "running", activity(card)));
  }
  const earlier = cards
    .filter((card) => card.closed && !working(card))
    .sort((a, b) => b.id - a.id)
    .slice(0, EARLIER);
  if (earlier.length > 0) {
    out.push({ type: "group", key: "group-earlier", name: "Earlier" });
    for (const card of earlier) out.push(entry(card, "done", "Done"));
  }
  return out;
}
