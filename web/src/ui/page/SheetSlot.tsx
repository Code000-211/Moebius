/**
 * The one place the two sheets share, over the page's right column: the shell holds which of them is showing, so the
 * Moebius agent and the package sheet never lie over each other. Opening one hides the other, which keeps its state; Escape and the
 * close control only reach the one that is showing.
 */
import { createContext, type ReactNode, useCallback, useContext, useMemo, useState } from "react";

/** The sheets that take the slot. */
export type SheetId = "chat" | "packages";

interface Slot {
  /** The sheet that is showing; none when both are hidden. */
  active: SheetId | null;
  /** Shows the sheet, hiding the one that was showing. */
  show: (id: SheetId) => void;
  /** Hides the sheet if it is the one showing. */
  hide: (id: SheetId) => void;
}

const EMPTY: Slot = { active: null, show: () => {}, hide: () => {} };

const Context = createContext<Slot>(EMPTY);

export function SheetSlotProvider({ children }: { children: ReactNode }) {
  const [active, setActive] = useState<SheetId | null>(null);
  const show = useCallback((id: SheetId) => setActive(id), []);
  const hide = useCallback((id: SheetId) => setActive((held) => (held === id ? null : held)), []);
  const value = useMemo(() => ({ active, show, hide }), [active, show, hide]);
  return <Context.Provider value={value}>{children}</Context.Provider>;
}

export const useSheetSlot = (): Slot => useContext(Context);
