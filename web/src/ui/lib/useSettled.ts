/** `value` once it has not changed for a short pause, so typing costs one search and not one per keystroke. */
import { useEffect, useState } from "react";

/** How long the typing waits before a search runs, in ms: a word arrives inside it, a letter does not. */
export const SETTLE = 250;

export function useSettled(value: string): string {
  const [settled, setSettled] = useState(value);
  useEffect(() => {
    const timer = setTimeout(() => setSettled(value), SETTLE);
    return () => clearTimeout(timer);
  }, [value]);
  return settled;
}
