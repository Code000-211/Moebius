/**
 * What is in focus for the palette. A page that has a card (or anything else) in focus renders `<PaletteFocusFor>`
 * with that thing's label and commands; the palette lists them first, and the keys the commands name work on the page
 * while the thing is in focus. The provider sits in the shell, so the palette and the keys see the same registration.
 */
import {
  createContext,
  type ReactNode,
  useContext,
  useEffect,
  useMemo,
  useRef,
  useState,
} from "react";
import { fieldHasKeys, modalIsOpen } from "../../../ui/lib/shortcuts";
import type { PaletteFocus } from "./model";

interface Registry {
  focus: PaletteFocus | null;
  set: (focus: PaletteFocus | null) => void;
}

const FocusContext = createContext<Registry>({ focus: null, set: () => {} });

export function PaletteFocusProvider({ children }: { children: ReactNode }) {
  const [focus, set] = useState<PaletteFocus | null>(null);
  const latest = useRef(focus);
  latest.current = focus;

  useEffect(() => {
    function onKey(event: KeyboardEvent) {
      if (event.defaultPrevented || event.ctrlKey || event.metaKey || event.altKey) return;
      if (modalIsOpen() || fieldHasKeys(event.target)) return;
      const command = latest.current?.commands.find((c) => c.key === event.key.toLowerCase());
      if (!command) return;
      event.preventDefault();
      command.run();
    }
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, []);

  const value = useMemo(() => ({ focus, set }), [focus]);
  return <FocusContext.Provider value={value}>{children}</FocusContext.Provider>;
}

/** What the page has in focus, or null. */
export function usePaletteFocus(): PaletteFocus | null {
  return useContext(FocusContext).focus;
}

/** Puts `focus` in the palette while this is on the page, and takes it out when it goes. */
export function PaletteFocusFor({ focus }: { focus: PaletteFocus }) {
  const { set } = useContext(FocusContext);
  useEffect(() => {
    set(focus);
  }, [focus, set]);
  useEffect(() => () => set(null), [set]);
  return null;
}
