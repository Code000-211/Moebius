/**
 * A scroller that says when it is cut off. The edges that have more content behind them fade out; an edge that shows
 * everything does not, because a fade that lies about hidden content is worse than none.
 *
 * It masks the scroller rather than laying a gradient over the top, so whatever the content is drawn on — a lane, a
 * card, the canvas — fades into what is behind the scroller instead of into a colour of its own.
 */
import {
  type CSSProperties,
  type ReactNode,
  useCallback,
  useEffect,
  useRef,
  useState,
} from "react";
import { cn } from "../../ui/lib/cn";

/** How far the ramp of a faded edge reaches into the content. */
const RAMP = "var(--space-24)";

/** A mask layer that hides nothing of this axis. */
const SOLID = "linear-gradient(currentColor, currentColor)";

/**
 * The mask of one axis: nothing at the start of the row where the start is cut off, nothing at its end where the end
 * is, and the whole of the row between.
 */
function axis(start: boolean, end: boolean, toStart: string, toEnd: string): string {
  if (start && end) {
    return `linear-gradient(${toEnd}, transparent, currentColor ${RAMP}, currentColor calc(100% - ${RAMP}), transparent)`;
  }
  if (start) return `linear-gradient(${toEnd}, transparent, currentColor ${RAMP})`;
  if (end) return `linear-gradient(${toStart}, transparent, currentColor ${RAMP})`;
  return SOLID;
}

/** Which edges of the box have more behind them: `scrollLeft` and `scrollTop` say where the view sits. */
function edgesOf(box: HTMLElement) {
  const overX = box.scrollWidth - box.clientWidth;
  const overY = box.scrollHeight - box.clientHeight;
  return {
    left: box.scrollLeft > 1,
    right: box.scrollLeft < overX - 1,
    top: box.scrollTop > 1,
    bottom: box.scrollTop < overY - 1,
  };
}

export function ScrollFades({ children, className }: { children: ReactNode; className?: string }) {
  const box = useRef<HTMLDivElement>(null);
  const [cut, setCut] = useState(edgesNone);
  // The edges only replace what is held when they differ. A box measured again would otherwise ask to render again,
  // and that render would measure again, without end — and a page that never settles never shows what a click asked
  // for, because the router waits for it to.
  const measure = useCallback(() => {
    const el = box.current;
    if (!el) return;
    const next = edgesOf(el);
    setCut((held) =>
      held.left === next.left &&
      held.right === next.right &&
      held.top === next.top &&
      held.bottom === next.bottom
        ? held
        : next,
    );
  }, []);

  const watch = useRef<ResizeObserver | null>(null);
  const watched = useRef(new Set<Element>());
  // Every render measures once more, once the DOM behind it is there, and takes on any child that arrived with the
  // cards — a lane added under a filter is a new element, and only a render knows about it. `measure` rests when
  // nothing moved, so a render that changed nothing costs one read of four numbers.
  useEffect(() => {
    measure();
    const el = box.current;
    if (!el || typeof ResizeObserver === "undefined") return;
    if (!watch.current) watch.current = new ResizeObserver(measure);
    const observer = watch.current;
    for (const each of [el, ...Array.from(el.children)]) {
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

  const style: CSSProperties = {
    maskImage: `${axis(cut.left, cut.right, "to left", "to right")}, ${axis(cut.top, cut.bottom, "to top", "to bottom")}`,
    WebkitMaskImage: `${axis(cut.left, cut.right, "to left", "to right")}, ${axis(cut.top, cut.bottom, "to top", "to bottom")}`,
    maskComposite: "intersect",
    WebkitMaskComposite: "source-in",
  };
  return (
    <div
      ref={box}
      onScroll={measure}
      style={style}
      className={cn("min-w-0 overflow-auto", className)}
      data-testid="scroll-fades"
    >
      {children}
    </div>
  );
}

const edgesNone = { left: false, right: false, top: false, bottom: false };
