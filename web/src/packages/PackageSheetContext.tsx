/**
 * Whether the package sheet is showing. The shell holds one of these, so there is one sheet at a time: a page opens it
 * (`open()` from the Plugins page), and it lies over the page's right column (`ui/page/SheetSlot.tsx`). An agent's
 * pi.dev packages are the Extensions tab of its page, not this sheet.
 */
import { createContext, type ReactNode, useCallback, useContext, useMemo, useState } from "react";
import { useSheetSlot } from "../ui/page/SheetSlot";

interface PackageSheet {
  /** Whether it was ever opened: until then there is no sheet. */
  opened: boolean;
  /** Whether it is the sheet showing. */
  shown: boolean;
  open: () => void;
  /** Hide it; its state stays until a page opens it again. */
  hide: () => void;
}

const NOWHERE: PackageSheet = { opened: false, shown: false, open: () => {}, hide: () => {} };

const Context = createContext<PackageSheet>(NOWHERE);

export function PackageSheetProvider({ children }: { children: ReactNode }) {
  const [opened, setOpened] = useState(false);
  const slot = useSheetSlot();
  const { show, hide: release } = slot;
  const shown = slot.active === "packages";
  const open = useCallback(() => {
    setOpened(true);
    show("packages");
  }, [show]);
  const hide = useCallback(() => release("packages"), [release]);
  const value = useMemo(() => ({ opened, shown, open, hide }), [opened, shown, open, hide]);
  return <Context.Provider value={value}>{children}</Context.Provider>;
}

export const usePackageSheet = (): PackageSheet => useContext(Context);
