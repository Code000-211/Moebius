/**
 * What the board page decides, as plain functions: which columns a board has, which cards each shows after the
 * filters, what state a card is in, what a drop of a card on a column means (and why it may be refused), and the
 * words and links of a card. `screens/board.rs` of the former desktop app is the model: the columns, the order, the ten cards
 * of Done and the card the dock shows without a choice are the same. The parts of this folder are one concern each; this
 * file re-exports their public items so that the page imports from `./model`.
 */

export type { ColumnCards } from "./columns";
export {
  boardColumns,
  COLUMNS,
  DONE_SHOWN,
  groupByColumn,
  orderCards,
} from "./columns";
export type { Drop } from "./drops";
export { dropRule } from "./drops";
export type { Filters } from "./filters";
export {
  agentsOf,
  DUE_WORDS,
  dueMatches,
  hasFilters,
  isPlanningLabel,
  labelsOf,
  matchesCard,
  NO_FILTERS,
  PRIORITY_WORDS,
  priorityRank,
  todayIso,
} from "./filters";
export { inboxOf, MERGE_STOPPED, mergeStopped, mergeStoppedOf } from "./inbox";
export type { Stack } from "./stacks";
export {
  descendants,
  isClosed,
  nextCard,
  parentNumber,
  parentOf,
  rootOf,
  stackWords,
} from "./stacks";
export type { CardStatus } from "./status";
export {
  cardStatus,
  changesOfCard,
  heldBy,
  heldLine,
  inRepository,
  isHeld,
  isLive,
  STATUS_LABELS,
} from "./status";
export type { CardBadge } from "./words";
export {
  cardBadges,
  cardDetail,
  changeOf,
  changeWorktree,
  deleteWords,
  describeHistory,
  forgeName,
  issueUrl,
  pullRequestOf,
  pullUrl,
  tokens,
  waitingLine,
} from "./words";
