//! The clock a page shows, kept moving while the page is open.

import { useEffect, useState } from "react";

/**
 * The epoch in seconds, read again every `everyMs`. Ages are said in words ("4 min ago"), which go stale on a page
 * nobody touches, so a page that says them keeps a timer of its own: half a minute where the ages are in minutes,
 * a few seconds where they are in seconds.
 */
export function useNow(everyMs = 30_000): number {
  const [now, setNow] = useState(() => Math.floor(Date.now() / 1000));
  useEffect(() => {
    const timer = setInterval(() => setNow(Math.floor(Date.now() / 1000)), everyMs);
    return () => clearInterval(timer);
  }, [everyMs]);
  return now;
}
