/**
 * Whether the Moebius agent's right panel is open, and what the docked field hands it. The shell holds one of these
 * for the whole window: the dock (`AgentDock.tsx`) asks through it, the panel (`AgentPanel.tsx`) reads it, and
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
  /** A message from the dock that the panel has not sent yet. */
  pending: string | null;
  /** Hands the message from the dock to the panel and opens it. */
  ask: (message: string) => void;
  /** Counts the fresh conversations asked for; the panel starts a new one each time it moves. */
  fresh: number;
  /** Opens the panel on a new, empty conversation. */
  newConversation: () => void;
  /** Takes the pending message; asked twice, the second answer is null. */
  take: () => string | null;
  show: () => void;
  /** Closes the panel; the dock takes the focus back when `refocus` is true. */
  hide: (refocus?: boolean) => void;
  /** Whether the dock should take the focus when it appears, which is once after the panel closed. */
  takeFocus: () => boolean;
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
  takeFocus: () => false,
};

const Context = createContext<Agent>(NOWHERE);

export function AgentProvider({ children }: { children: ReactNode }) {
  const [open, setOpen] = useState(false);
  const [built, setBuilt] = useState(false);
  const [fresh, setFresh] = useState(0);
  const [pending, setPending] = useState<string | null>(null);
  const held = useRef<string | null>(null);
  const refocus = useRef(false);

  const show = useCallback(() => {
    setBuilt(true);
    setOpen(true);
  }, []);
  const hide = useCallback((back = false) => {
    refocus.current = back;
    setOpen(false);
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
  const takeFocus = useCallback(() => {
    const back = refocus.current;
    refocus.current = false;
    return back;
  }, []);

  // `Ctrl+J` opens the agent and closes it again, from anywhere but a modal dialog, which keeps the keyboard to itself.
  useEffect(() => {
    function onKey(event: KeyboardEvent) {
      if (!isShortcut(event, "j") || modalIsOpen()) return;
      event.preventDefault();
      if (open) hide();
      else show();
    }
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, [open, show, hide]);

  const value = useMemo(
    () => ({ open, built, pending, ask, fresh, newConversation, take, show, hide, takeFocus }),
    [open, built, pending, ask, fresh, newConversation, take, show, hide, takeFocus],
  );
  return <Context.Provider value={value}>{children}</Context.Provider>;
}

export const useAgent = (): Agent => useContext(Context);
