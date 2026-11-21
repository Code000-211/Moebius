/**
 * When the window is too narrow for the sidebar. Below `NARROW_BELOW` pixels the sidebar leaves the layout and lies
 * over the page when a person asks for it with the button in the page's header; at that width and above it stays where
 * it is. The rule is the viewport's width alone, so nothing is stored.
 */
import { useEffect, useState } from "react";

/** The width at which the sidebar stays: 999 is narrow, 1000 is wide. */
export const NARROW_BELOW = 1000;

/** Whether a window of `width` pixels is too narrow for the sidebar. */
export function isNarrow(width: number): boolean {
  return width < NARROW_BELOW;
}

/** Whether the window is narrow now, kept up to date while it is resized. */
export function useNarrow(): boolean {
  const [narrow, setNarrow] = useState(() => isNarrow(window.innerWidth));
  useEffect(() => {
    const update = () => setNarrow(isNarrow(window.innerWidth));
    update();
    window.addEventListener("resize", update);
    return () => window.removeEventListener("resize", update);
  }, []);
  return narrow;
}
