/**
 * What a person has chosen on a page of rows or cards: a set of keys, kept in the order the page shows them. A plain
 * click on a choice box toggles one, a shift-click chooses everything between it and the last one touched, and Escape
 * clears the choice while no field, menu or dialog has the keys. A key whose row has left the page (moved, filtered
 * out, gone) is not chosen any more, so a bar never counts what cannot be seen.
 *
 * ```ts
 * const choice = useSelection(cards.map((card) => card.id));
 * choice.toggle(card.id, event.shiftKey);
 * choice.keys; // the chosen ids, in page order
 * ```
 */
import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import { fieldHasKeys, modalIsOpen } from "./shortcuts";

export interface Selection<K> {
  /** The chosen keys, in the order of the page. */
  keys: K[];
  has: (key: K) => boolean;
  /** Chooses or lets go of one; with `range`, chooses all from the last one touched to this one. */
  toggle: (key: K, range?: boolean) => void;
  /** Makes `key` the whole choice, unless it is chosen already (a right click on a row keeps the choice it is in). */
  ensure: (key: K) => void;
  clear: () => void;
}

export function useSelection<K>(order: readonly K[]): Selection<K> {
  const [chosen, setChosen] = useState<ReadonlySet<K>>(new Set());
  const anchor = useRef<K | null>(null);
  const latest = useRef(order);
  latest.current = order;
  const keys = useMemo(() => order.filter((key) => chosen.has(key)), [order, chosen]);

  const toggle = useCallback((key: K, range = false) => {
    setChosen((now) => {
      const next = new Set(now);
      const from = anchor.current === null ? -1 : latest.current.indexOf(anchor.current);
      const to = latest.current.indexOf(key);
      anchor.current = key;
      if (range && from >= 0 && to >= 0) {
        for (let at = Math.min(from, to); at <= Math.max(from, to); at++) {
          const between = latest.current[at];
          if (between !== undefined) next.add(between);
        }
      } else if (next.has(key)) next.delete(key);
      else next.add(key);
      return next;
    });
  }, []);
  const ensure = useCallback((key: K) => {
    setChosen((now) => (now.has(key) ? now : new Set([key])));
    anchor.current = key;
  }, []);
  const clear = useCallback(() => {
    setChosen(new Set());
    anchor.current = null;
  }, []);
  const has = useCallback((key: K) => chosen.has(key), [chosen]);

  const any = keys.length > 0;
  useEffect(() => {
    if (!any) return;
    function onKey(event: KeyboardEvent) {
      if (event.key !== "Escape" || event.defaultPrevented) return;
      if (modalIsOpen() || fieldHasKeys(event.target)) return;
      event.preventDefault();
      clear();
    }
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, [any, clear]);

  return { keys, has, toggle, ensure, clear };
}
