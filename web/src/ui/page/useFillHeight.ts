/**
 * The height a virtual list needs when its page scrolls as a whole: from where the list starts to the foot of the
 * scrolling area it is in (less the page's padding), so the list fills what is left and scrolls inside itself. It
 * follows the window's size, and keeps a minimum so a short window still shows a few rows.
 */
import { useCallback, useLayoutEffect, useState } from "react";

const MIN = 256;
const PADDING = 24;

export function useFillHeight(): { ref: (node: HTMLElement | null) => void; height: number } {
  const [node, setNode] = useState<HTMLElement | null>(null);
  const [height, setHeight] = useState(MIN);
  const ref = useCallback((el: HTMLElement | null) => setNode(el), []);
  useLayoutEffect(() => {
    if (!node) return;
    const measure = () => {
      let area: HTMLElement | null = node.parentElement;
      while (area && !/(auto|scroll)/.test(getComputedStyle(area).overflowY))
        area = area.parentElement;
      const bottom = area ? area.getBoundingClientRect().bottom : window.innerHeight;
      setHeight(Math.max(MIN, Math.floor(bottom - node.getBoundingClientRect().top - PADDING)));
    };
    measure();
    window.addEventListener("resize", measure);
    return () => window.removeEventListener("resize", measure);
  }, [node]);
  return { ref, height };
}
