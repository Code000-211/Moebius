/**
 * Which inbox items this browser has read, so an item loses its unread dot once it was chosen. It is a per-viewer
 * convenience kept in localStorage (`moebius.inbox.read`, a list of item ids, the newest last and at most
 * `KEPT` long), so a blocked or empty store only brings the dots back.
 */
import { useCallback, useState } from "react";
import { readText, writeText } from "../storage";

const KEY = "moebius.inbox.read";
/** How many ids are kept: far more than an inbox holds, so the list does not grow for ever. */
const KEPT = 500;

/** The ids kept as read; nothing for a store that is empty, blocked or holds something else. */
export function readIds(): number[] {
  const kept = readText(KEY);
  if (kept === null) return [];
  try {
    const list: unknown = JSON.parse(kept);
    return Array.isArray(list) ? list.filter((id): id is number => typeof id === "number") : [];
  } catch {
    return [];
  }
}

/** `id` added to `ids` as the newest, and the oldest dropped past the limit. */
export function withRead(ids: readonly number[], id: number): number[] {
  return [...ids.filter((each) => each !== id), id].slice(-KEPT);
}

/** The ids read so far, and the way to read one more. */
export function useRead(): { read: ReadonlySet<number>; markRead: (id: number) => void } {
  const [ids, setIds] = useState(readIds);
  const markRead = useCallback((id: number) => {
    setIds((before) => {
      if (before.includes(id)) return before;
      const next = withRead(before, id);
      writeText(KEY, JSON.stringify(next));
      return next;
    });
  }, []);
  return { read: new Set(ids), markRead };
}
