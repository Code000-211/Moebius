/**
 * The reduce-motion switch of the web app. A person who turns it on gets no transition and no animation
 * (`docs/design-web.md`, Motion): the choice is kept in local storage like the theme, and written on
 * `<html data-reduce-motion>` where `src/styles/tailwind.css` reads it, with the same rules as the
 * `prefers-reduced-motion` media query, which keeps working on its own.
 *
 * The script in `public/theme-init.js` sets the attribute before the first picture; `useReduceMotion` is how a page
 * offers the switch. A browser that blocks storage still obeys the switch for the page's lifetime.
 */
import { useCallback, useSyncExternalStore } from "react";

const key = "moebius.reduce-motion";

/** What was switched in this page, kept in memory in case storage refuses it. */
let chosen: boolean | null = null;

function saved(): boolean {
  if (chosen !== null) return chosen;
  try {
    return localStorage.getItem(key) === "on";
  } catch {
    return false;
  }
}

/** Whether the system asks for less motion; the switch then shows on and cannot be turned off from here. */
export function systemReducesMotion(): boolean {
  return (
    typeof matchMedia !== "undefined" && matchMedia("(prefers-reduced-motion: reduce)").matches
  );
}

function apply(on: boolean): void {
  if (typeof document === "undefined") return;
  if (on) document.documentElement.dataset.reduceMotion = "true";
  else delete document.documentElement.dataset.reduceMotion;
}

/** Called by `main.tsx` before the app draws. */
export function start(): boolean {
  const on = saved();
  apply(on);
  return on;
}

const listeners = new Set<() => void>();

function set(on: boolean): void {
  chosen = on;
  apply(on);
  try {
    localStorage.setItem(key, on ? "on" : "off");
  } catch {
    // The switch is in effect and simply not remembered for the next visit.
  }
  for (const listener of listeners) listener();
}

/** Whether the person turned reduce motion on, and the way to change it. */
export function useReduceMotion(): { on: boolean; system: boolean; setOn: (on: boolean) => void } {
  const on = useSyncExternalStore(
    (listener) => {
      listeners.add(listener);
      return () => listeners.delete(listener);
    },
    saved,
    saved,
  );
  const setOn = useCallback((next: boolean) => set(next), []);
  return { on, system: systemReducesMotion(), setOn };
}
