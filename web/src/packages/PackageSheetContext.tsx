/**
 * Where the package sheet is pointed and whether it is showing. The shell holds one of these, so there is one sheet at
 * a time: a page opens it on a catalog (`open("moebius")` from the Plugins page, `open("pi", agent)` from an agent's
 * Extensions), and opening it from elsewhere switches the catalog it shows. It shares its slot with the floating chat
 * (`ui/page/SheetSlot.tsx`): opening one minimises the other. The shell also reads `shown` to leave the sheet its room
 * on a wide window, so the page it was opened from is not covered.
 */
import { createContext, type ReactNode, useCallback, useContext, useMemo, useState } from "react";
import { useSheetSlot } from "../ui/page/SheetSlot";
import type { Catalog } from "./model";

interface Pointed {
  catalog: Catalog;
  /** The agent a Pi package is proposed for; none when the sheet was not opened from an agent. */
  agent: string | null;
  /** Whether it was ever opened: until then there is no sheet and no button for it. */
  opened: boolean;
}

interface PackageSheet extends Pointed {
  /** Whether it is the sheet showing (the chat is minimised then). */
  shown: boolean;
  open: (catalog: Catalog, agent?: string | null) => void;
  /** Minimise it to its button; its state stays until it is opened again. */
  hide: () => void;
}

const POINTED: Pointed = { catalog: "moebius", agent: null, opened: false };

const NOWHERE: PackageSheet = {
  ...POINTED,
  shown: false,
  open: () => {},
  hide: () => {},
};

const Context = createContext<PackageSheet>(NOWHERE);

export function PackageSheetProvider({ children }: { children: ReactNode }) {
  const [pointed, setPointed] = useState<Pointed>(POINTED);
  const slot = useSheetSlot();
  const { show, hide: release } = slot;
  const shown = slot.active === "packages";
  const open = useCallback(
    (catalog: Catalog, agent: string | null = null) => {
      setPointed({ catalog, agent: catalog === "pi" ? agent : null, opened: true });
      show("packages");
    },
    [show],
  );
  const hide = useCallback(() => release("packages"), [release]);
  const value = useMemo(() => ({ ...pointed, shown, open, hide }), [pointed, shown, open, hide]);
  return <Context.Provider value={value}>{children}</Context.Provider>;
}

export const usePackageSheet = (): PackageSheet => useContext(Context);
