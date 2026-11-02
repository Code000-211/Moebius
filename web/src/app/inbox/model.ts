/**
 * The logic of the Inbox page, apart from React: what an item is about (looked up in the snapshot), its kind as a tag
 * and a sentence, the facts of a change that waits, the filters and their counts, and the list of issues a grilling
 * proposes while a person edits it (the desktop app's `screens/inbox`).
 */
import type { Board, Card, Change, InboxItem, Snapshot } from "../../api/types/board";
import type { Proposal } from "../../api/types/improve";
import type { GrillingIssue } from "../../api/types/inbox";
import type { Tone } from "../../ui/page/Status";
import { tokens } from "../board/model";

export const READY_TO_MERGE = "ready_to_merge";
export const APPROVE_ISSUE = "approve_issue";
export const STUCK = "stuck";
export const CHANGE_CLOSED = "change_closed";
export const PROPOSAL = "proposal";
export const TAKEOVER = "takeover";
export const GRILLING_ISSUES = "grilling_issues";
export const GRILLING_REPORT = "grilling_report";

/** The kinds the core adds, in the order the filter lists them; kinds of column scripts follow. */
export const KINDS = [
  READY_TO_MERGE,
  APPROVE_ISSUE,
  STUCK,
  CHANGE_CLOSED,
  PROPOSAL,
  TAKEOVER,
  GRILLING_ISSUES,
  GRILLING_REPORT,
] as const;

/** The branch of a change that offers the work of a chat is `moebius/chat-<id>`. */
const CHAT_BRANCH = "moebius/chat-";

const number = (value: unknown): number | null => (typeof value === "number" ? value : null);
const text = (value: unknown): string => (typeof value === "string" ? value : "");

/** The id of the change a `ready_to_merge` or `change_closed` item is about. */
export const changeOf = (item: InboxItem) => number(item.data.change);
export const proposalOf = (item: InboxItem) => number(item.data.proposal);
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
}

export function found(snapshot: Snapshot, item: InboxItem): Found {
  const changeId = changeOf(item);
  const board = snapshot.boards.find(
    (each) =>
      each.cards.some((card) => card.id === item.card_id) ||
      each.changes.some((change) => change.id === changeId) ||
      ((grillingOf(item) !== null || item.kind === PROPOSAL) &&
        repositoryOf(item) === each.repository.id),
  );
  return {
    board,
    card: board?.cards.find((card) => card.id === item.card_id),
    change:
      board?.changes.find((change) => change.id === changeId) ??
      snapshot.harness_changes.find((change) => change.id === changeId),
  };
}

/** A kind of a column script, such as `needs_design`, as a label: `Needs design`. */
export function sentence(kind: string): string {
  const words = kind.replaceAll("_", " ");
  return words.charAt(0).toUpperCase() + words.slice(1);
}

/** The name of a kind on its filter chip. */
export function chipName(kind: string): string {
  switch (kind) {
    case READY_TO_MERGE:
      // The board's own column name, so the chip and the card the chip filters to say one thing.
      return "Ready to merge";
    case APPROVE_ISSUE:
      return "Approvals";
    case STUCK:
      return "Stuck";
    case CHANGE_CLOSED:
      return "Closed";
    case PROPOSAL:
      return "Proposals";
    case TAKEOVER:
      return "Takeovers";
    case GRILLING_ISSUES:
      return "Grilled issues";
    case GRILLING_REPORT:
      return "Reports";
    default:
      return sentence(kind);
  }
}

/**
 * What the item is, and how loud: the colour of an error for what is blocked, of attention for what waits for the
 * person's move. The tones are the shared pill's (`ui/page/Status`), so the word and its hue mean the same thing here
 * as they do on a card and on a run.
 */
export function kindTag(item: InboxItem): { label: string; tone: Tone } {
  switch (item.kind) {
    case STUCK:
      return { label: "Stuck", tone: "error" };
    case READY_TO_MERGE:
      return item.card_id === null
        ? { label: "Change from a chat", tone: "attention" }
        : { label: "Ready to merge", tone: "attention" };
    case APPROVE_ISSUE:
      return { label: "Outside issue", tone: "attention" };
    case CHANGE_CLOSED:
      return { label: "Closed without a merge", tone: "attention" };
    case PROPOSAL:
      return { label: "Retro proposal", tone: "attention" };
    case TAKEOVER:
      return { label: "Decision model", tone: "attention" };
    case GRILLING_ISSUES:
      return { label: "Issues from a grilling", tone: "attention" };
    case GRILLING_REPORT:
      return { label: "Report from a grilling", tone: "attention" };
    default:
      return { label: sentence(item.kind), tone: "neutral" };
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
    case TAKEOVER: {
      const agreement = Math.round(
        (typeof item.data.agreement === "number" ? item.data.agreement : 0) * 100,
      );
      const count = number(item.data.count) ?? 0;
      return `It chose what was recorded on ${agreement}% of ${count} judgements held back from training. Merging lets it decide this kind from now on.`;
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
  if (about.board) parts.push(about.board.repository.name);
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
}

export const NO_FILTERS: Filters = { kind: null, repository: null };

const inKind = (filters: Filters, item: InboxItem) =>
  filters.kind === null || filters.kind === item.kind;

const inRepository = (snapshot: Snapshot, repository: number | null, item: InboxItem) =>
  repository === null || found(snapshot, item).board?.repository.id === repository;

/** The items the filters let through, newest first. */
export function shown(snapshot: Snapshot, filters: Filters): InboxItem[] {
  return snapshot.inbox
    .filter((item) => inKind(filters, item) && inRepository(snapshot, filters.repository, item))
    .sort((a, b) => b.at - a.at || b.id - a.id);
}

/** How many items a chip stands for: those of a kind (any when null) in a repository (any when null). */
export function count(snapshot: Snapshot, kind: string | null, repository: number | null): number {
  return snapshot.inbox.filter(
    (item) => inKind({ kind, repository: null }, item) && inRepository(snapshot, repository, item),
  ).length;
}

/** The kinds on the chips: the core's, in order, then those of column scripts by name; only those with an item. */
export function kindsOf(snapshot: Snapshot): string[] {
  const present = new Set(snapshot.inbox.map((item) => item.kind));
  const core = KINDS.filter((kind) => present.has(kind));
  const others = [...present].filter((kind) => !(KINDS as readonly string[]).includes(kind)).sort();
  return [...core, ...others];
}

/** What the page says under its title. */
export function summary(total: number, shownCount: number): string {
  // An empty inbox is said by the empty state below it, which says it better.
  if (total === 0) return "";
  if (total === 1) return "1 thing needs you.";
  if (total === shownCount) return `${total} things need you.`;
  return `${total} things need you; ${shownCount} shown.`;
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
