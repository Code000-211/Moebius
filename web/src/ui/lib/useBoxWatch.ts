/**
 * A box that says something about its own content — which of its edges are cut off, whether a line ran out, whether a
 * paragraph was cut short — has to go on saying it when the content or the width changes under it. This is the
 * watching that asks again: every render asks once more, once the DOM behind it is there, and the box is watched for a
 * change of size, so a column that narrows or a card that gains a line is measured again rather than kept to the
 * answer it was given once.
 *
 * `measure` reads the box and is free to be written anew each render, since what the observer holds is the reading, not
 * the function. It must only ask to render when its answer changes: a box measured again would otherwise ask to render
 * again, and that render would measure again, without end — and a page that never settles never shows what a click
 * asked for, because the router waits for it to.
 */
import { type RefObject, useEffect, useLayoutEffect, useRef } from "react";

/** What a box watched by {@link useBoxWatch} hands out: the ref for the box, and the callback for its `onScroll`. */
export interface BoxWatch<T> {
  box: RefObject<T | null>;
  /** Read the box again now — the handler for a scroll, which changes what the box is showing. */
  remeasure: () => void;
}

/**
 * Watch `measure` over the box it is given. `children` adds every child the box has, for a box whose answer depends on
 * the rows inside it rather than on the box alone; a child that arrives later is added by the render that brought it, and one that left is let go by the next.
 */
export function useBoxWatch<T extends HTMLElement>(
  measure: (box: T) => void,
  { children = false }: { children?: boolean } = {},
): BoxWatch<T> {
  const box = useRef<T | null>(null);
  const sink = useRef(measure);
  const watch = useRef<ResizeObserver | null>(null);
  const watched = useRef(new Set<Element>());

  const remeasure = () => {
    const el = box.current;
    if (el) sink.current(el);
  };

  // No deps: every render reads the box once more, and takes on any child that arrived with it.
  useLayoutEffect(() => {
    sink.current = measure;
    remeasure();
    const el = box.current;
    if (!el || typeof ResizeObserver === "undefined") return;
    if (!watch.current) watch.current = new ResizeObserver(remeasure);
    const observer = watch.current;
    const seen = children ? [el, ...Array.from(el.children)] : [el];
    // A child that left the box is no longer watched, and the set no longer holds on to it and what it holds.
    for (const each of Array.from(watched.current)) {
      if (seen.includes(each)) continue;
      observer.unobserve(each);
      watched.current.delete(each);
    }
    for (const each of seen) {
      if (watched.current.has(each)) continue;
      watched.current.add(each);
      observer.observe(each);
    }
  });
  useEffect(
    () => () => {
      watch.current?.disconnect();
      watch.current = null;
      watched.current.clear();
    },
    [],
  );

  return { box, remeasure };
}
