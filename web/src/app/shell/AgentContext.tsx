/**
 * Whether the Moebius agent's right panel is open, and what the command bar hands it. The shell holds one of these
 * for the whole window: the header's Moebius button (`AgentButton.tsx`) toggles it, the command bar asks through it, the panel (`AgentPanel.tsx`) reads it, and
 * `Ctrl+J` toggles it. The panel is first built when the agent is first asked or opened, and stays built after that, so
 * its conversation survives being closed.
 */
import {
  createContext,
  type ReactNode,
  useCallback,
  useContext,
  useEffect,
  useMemo,
  useRef,
  useState,
} from "react";
import { isShortcut, modalIsOpen } from "../../ui/lib/shortcuts";

interface Agent {
  /** Whether the panel is open. */
  open: boolean;
  /** Whether the panel was ever opened, which is when it is built. */
  built: boolean;
  /** A message from the command bar that the panel has not sent yet. */
  pending: string | null;
  /** Hands the message to the panel and opens it. */
  ask: (message: string) => void;
  /** Counts the fresh conversations asked for; the panel starts a new one each time it moves. */
  fresh: number;
  /** Opens the panel on a new, empty conversation. */
  newConversation: () => void;
  /** Takes the pending message; asked twice, the second answer is null. */
  take: () => string | null;
  show: () => void;
  /** Closes the panel; the header's button takes the focus back. */
  hide: () => void;
  /** Opens the panel when it is closed and closes it when it is open. */
  toggle: () => void;
}

const NOWHERE: Agent = {
  open: false,
  built: false,
  pending: null,
  ask: () => {},
  fresh: 0,
  newConversation: () => {},
  take: () => null,
  show: () => {},
  hide: () => {},
  toggle: () => {},
};

const Context = createContext<Agent>(NOWHERE);

export function AgentProvider({ children }: { children: ReactNode }) {
  const [open, setOpen] = useState(false);
  const [built, setBuilt] = useState(false);
  const [fresh, setFresh] = useState(0);
  const [pending, setPending] = useState<string | null>(null);
  const held = useRef<string | null>(null);

  const show = useCallback(() => {
    setBuilt(true);
    setOpen(true);
  }, []);
  const hide = useCallback(() => setOpen(false), []);
  const toggle = useCallback(() => {
    setBuilt(true);
    setOpen((was) => !was);
  }, []);
  const ask = useCallback(
    (message: string) => {
      held.current = message;
      setPending(message);
      show();
    },
    [show],
  );
  const newConversation = useCallback(() => {
    held.current = null;
    setPending(null);
    setFresh((n) => n + 1);
    show();
  }, [show]);
  const take = useCallback(() => {
    const message = held.current;
    held.current = null;
    setPending(null);
    return message;
  }, []);

  // `Ctrl+J` opens the agent and closes it again, from anywhere but a modal dialog, which keeps the keyboard to itself.
  useEffect(() => {
    function onKey(event: KeyboardEvent) {
      if (!isShortcut(event, "j") || modalIsOpen()) return;
      event.preventDefault();
      if (open) hide();
      else show();
    }
    // In the capture phase, so no control that keeps its keys from the page can swallow the shortcut.
    window.addEventListener("keydown", onKey, true);
    return () => window.removeEventListener("keydown", onKey, true);
  }, [open, show, hide]);

  const value = useMemo(
    () => ({ open, built, pending, ask, fresh, newConversation, take, show, hide, toggle }),
    [open, built, pending, ask, fresh, newConversation, take, show, hide, toggle],
  );
  return <Context.Provider value={value}>{children}</Context.Provider>;
}

export const useAgent = (): Agent => useContext(Context);
