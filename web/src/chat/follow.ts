/**
 * Whether the conversation follows what the agent writes. It does while the person is at its end, stops when they
 * scroll up, and starts again when they reach the end or send a message; opening a step of the conversation stops it
 * too, because that is what a person goes to read. Content that grows moves no `scrollTop`, but
 * the list that draws only what is on screen sometimes does (it measures a taller turn and corrects its position),
 * so a `scrollTop` that went down stops the following only when the person did it: with the wheel, a finger, a key or
 * the scrollbar, shortly before.
 */

/** How long after a gesture of the person a scroll is theirs, in milliseconds. */
export const GESTURE_WINDOW_MS = 500;

/** Whether a scroll at `now` was the person's, given when their last gesture on the list was (null for none). */
export function byPerson(lastGesture: number | null, now: number): boolean {
  return lastGesture !== null && now - lastGesture < GESTURE_WINDOW_MS;
}

/** How far from the end still counts as at the end, in pixels. */
export const END_SLACK = 24;

export interface ScrollState {
  follow: boolean;
  /** The `scrollTop` seen last. */
  top: number;
}

export interface Scrolled {
  top: number;
  height: number;
  viewport: number;
}

/** Whether a scrolled viewport shows the end of the content, give or take a line. */
export function atEnd({ top, height, viewport }: Scrolled): boolean {
  return height - (top + viewport) <= END_SLACK;
}

/** The state after a scroll event; `person` is whether the person made it (see `byPerson`). */
export function afterScroll(state: ScrollState, scrolled: Scrolled, person: boolean): ScrollState {
  if (atEnd(scrolled)) return { follow: true, top: scrolled.top };
  if (person && scrolled.top < state.top) return { follow: false, top: scrolled.top };
  return { follow: state.follow, top: scrolled.top };
}

/** Whether a following list that is not at its end is to be scrolled on: the list moved it, not the person. */
export function shouldRepin(state: ScrollState, scrolled: Scrolled): boolean {
  return state.follow && !atEnd(scrolled);
}

/** What sending a message does: the conversation follows again, wherever it was scrolled to. */
export function afterSend(state: ScrollState): ScrollState {
  return { ...state, follow: true };
}

/**
 * What opening or closing a step of the conversation does: the end is let go of. A person who unfolds a thought is
 * reading what is above the last line, and a list pinned to its end scrolls that out of the way the moment it grows.
 * Scrolling to the end, or sending a message, takes the end again.
 */
export function afterToggle(state: ScrollState): ScrollState {
  return { ...state, follow: false };
}

/**
 * How many turns arrived above the one that was first: `was` is the key of the turn at the top of the list before the
 * conversation grew, and the answer is where that turn is now. `null` says the turn is gone, which is a different
 * conversation rather than an older page of this one, and the list starts counting from zero again.
 */
export function prepended(turns: readonly { key: string }[], was: string | null): number | null {
  if (was === null) return 0;
  const at = turns.findIndex((turn) => turn.key === was);
  return at < 0 ? null : at;
}
