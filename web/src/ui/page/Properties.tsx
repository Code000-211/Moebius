/**
 * A page's properties, drawn in the right column of the page's surface rather than in the page (ADR 0020). The shell
 * holds a `PropertiesProvider` and gives it the column's body to draw into; a page puts what it wants there with
 * `<PageProperties>`, which stays a part of the page's own tree — its state lives and dies with the page — while its
 * boxes appear in the column. The column knows from `useProperties().present` whether the page has any, which decides
 * whether it may unfold, and says back with `setShown` whether it is unfolded, which a page that lays something beside
 * the column reads with `usePropertiesShown()`. Outside a shell (a page's own test) the properties are drawn where the
 * page put them.
 */
import {
  createContext,
  type ReactNode,
  useCallback,
  useContext,
  useLayoutEffect,
  useMemo,
  useState,
} from "react";
import { createPortal } from "react-dom";

interface Column {
  /** The element the properties are drawn into; null until the column has drawn it. */
  target: HTMLElement | null;
  setTarget: (element: HTMLElement | null) => void;
  /** Whether the page on screen put any properties in the column. */
  present: boolean;
  /** Says a page's properties are in the column; the function it returns takes them out again. */
  hold: () => () => void;
  /** Whether the column is unfolded with the properties in it, as the column says. */
  shown: boolean;
  setShown: (shown: boolean) => void;
}

const Context = createContext<Column | null>(null);

export function PropertiesProvider({ children }: { children: ReactNode }) {
  const [target, setTarget] = useState<HTMLElement | null>(null);
  const [held, setHeld] = useState(0);
  const [shown, setShown] = useState(false);
  const hold = useCallback(() => {
    setHeld((count) => count + 1);
    return () => setHeld((count) => count - 1);
  }, []);
  const value = useMemo(
    () => ({ target, setTarget, present: held > 0, hold, shown, setShown }),
    [target, held, hold, shown],
  );
  return <Context.Provider value={value}>{children}</Context.Provider>;
}

/** The column's side of it: where to draw, whether the page has anything to draw there, and saying it is unfolded. */
export function useProperties(): {
  present: boolean;
  setTarget: (element: HTMLElement | null) => void;
  setShown: (shown: boolean) => void;
} {
  const column = useContext(Context);
  return {
    present: column?.present ?? false,
    setTarget: column?.setTarget ?? (() => {}),
    setShown: column?.setShown ?? (() => {}),
  };
}

/** Whether the right column stands unfolded with the page's properties, so what the page lays beside it can make room. */
export function usePropertiesShown(): boolean {
  return useContext(Context)?.shown ?? false;
}

/** What a page puts in the right column: its properties, named for a person hearing the page by `label`. */
export function PageProperties({ label, children }: { label: string; children: ReactNode }) {
  const column = useContext(Context);
  const hold = column?.hold;
  // Before paint, so the column unfolds in the same frame the page appears in rather than one after it.
  useLayoutEffect(() => hold?.(), [hold]);
  const body = (
    <section aria-label={label} className="flex flex-col gap-1 px-3 py-2">
      {children}
    </section>
  );
  if (column === null) return body;
  return column.target ? createPortal(body, column.target) : null;
}

/** One property: its name, and its value beside it. */
export function Property({ name, children }: { name: string; children: ReactNode }) {
  return (
    <div className="grid min-h-8 grid-cols-[6rem_minmax(0,1fr)] items-center gap-2 px-1 text-sm">
      <span className="text-secondary">{name}</span>
      <span className="min-w-0 break-words text-text">{children}</span>
    </div>
  );
}
