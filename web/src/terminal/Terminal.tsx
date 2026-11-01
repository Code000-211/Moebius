/**
 * A terminal of the server in the page: xterm.js on a WebSocket (`protocol.ts` says what goes over it). What a person
 * types goes to the shell as bytes, what the shell prints is written to the screen, and the size in cells goes back as
 * a resize whenever the pane is fitted to its box. Like the desktop app's pane (`widgets/terminal/pane.rs`), a
 * selection is copied when the mouse is let go and with Ctrl+Shift+C, and the clipboard is pasted with Ctrl+Shift+V and
 * with a right click; xterm brackets a paste when the program asked for it. Closing the page or leaving it closes the
 * socket, which kills the shell. The pane itself is exempt from the app's type scale, since it shows characters at a
 * fixed size; the status line and the controls around it are not, and take their colours from the tokens of whichever
 * theme is on, as the pane does.
 */
import "@xterm/xterm/css/xterm.css";
import { FitAddon } from "@xterm/addon-fit";
import { WebLinksAddon } from "@xterm/addon-web-links";
import { Terminal as XTerm } from "@xterm/xterm";
import { useEffect, useRef, useState } from "react";
import { useTheme } from "../theme";
import { Button } from "../ui/components/Button";
import { cn } from "../ui/lib/cn";
import {
  binaryBytes,
  endMessage,
  inputBytes,
  outputBytes,
  resizeMessage,
  terminalUrl,
} from "./protocol";

type Status = "connecting" | "open" | "ended";

/**
 * The colours of the terminal, read from `styles/theme.css` by name so the pane has no colours of its own. A token
 * that is not there leaves xterm on its own default rather than guessing, so the app carries no colour literal.
 */
function themeOf() {
  const style = getComputedStyle(document.documentElement);
  const named = (name: string) => style.getPropertyValue(name).trim() || undefined;
  return {
    background: named("--terminal"),
    foreground: named("--terminal-text"),
    cursor: named("--terminal-cursor"),
    selectionBackground: named("--terminal-selection"),
  };
}

interface TerminalProps {
  /** The terminal's route, such as `/v1/terminal` or `/v1/runs/3/terminal`. */
  path: string;
  /** Takes the keyboard when the terminal opens. */
  autoFocus?: boolean;
  /**
   * Gives the keyboard to the pane whenever this turns true. A shell kept mounted while the sheet showing it is
   * closed is never opened again — the socket has been up the whole time — so `autoFocus` has had its say and the
   * sheet has to ask a second time.
   */
  focused?: boolean;
  /** Called once when the shell has ended or could not be opened. */
  onEnded?: (opened: boolean) => void;
  className?: string;
}

export function Terminal({ path, autoFocus, focused, onEnded, className }: TerminalProps) {
  const host = useRef<HTMLDivElement>(null);
  const pane = useRef<XTerm | null>(null);
  const [status, setStatus] = useState<Status>("connecting");
  const [opened, setOpened] = useState(false);
  const [attempt, setAttempt] = useState(0);
  const ended = useRef(onEnded);
  ended.current = onEnded;
  const { theme } = useTheme();

  // biome-ignore lint/correctness/useExhaustiveDependencies: `attempt` makes a new shell on the same path
  useEffect(() => {
    const element = host.current;
    if (!element) return;
    setStatus("connecting");
    setOpened(false);
    const term = new XTerm({
      fontFamily: '"JetBrains Mono", ui-monospace, monospace',
      fontSize: 13,
      cursorBlink: true,
      scrollback: 5000,
      theme: themeOf(),
    });
    pane.current = term;
    const fit = new FitAddon();
    term.loadAddon(fit);
    term.loadAddon(new WebLinksAddon());
    term.open(element);
    try {
      fit.fit();
    } catch {
      // A box without a size yet is fitted by the observer below.
    }

    const socket = new WebSocket(terminalUrl(window.location, path));
    socket.binaryType = "arraybuffer";
    let wasOpen = false;
    let over = false;
    const send = (data: string | Uint8Array<ArrayBuffer>) => {
      if (socket.readyState === WebSocket.OPEN) socket.send(data);
    };
    const sendSize = () => {
      const message = resizeMessage(term.cols, term.rows);
      if (message !== null) send(message);
    };
    const copy = () => {
      if (term.hasSelection())
        void navigator.clipboard?.writeText(term.getSelection()).catch(() => {});
    };
    const paste = () => {
      void navigator.clipboard
        ?.readText()
        .then((text) => {
          if (text !== "") term.paste(text);
        })
        .catch(() => {});
    };

    socket.onopen = () => {
      wasOpen = true;
      setOpened(true);
      setStatus("open");
      sendSize();
      if (autoFocus) term.focus();
    };
    socket.onmessage = (event) => {
      const bytes = outputBytes(event.data);
      if (bytes !== null) term.write(bytes);
    };
    socket.onclose = () => {
      if (over) return;
      over = true;
      setStatus("ended");
      ended.current?.(wasOpen);
    };

    const subscriptions = [
      term.onData((text) => send(inputBytes(text) as Uint8Array<ArrayBuffer>)),
      term.onBinary((binary) => send(binaryBytes(binary) as Uint8Array<ArrayBuffer>)),
      term.onResize(sendSize),
    ];
    term.attachCustomKeyEventHandler((event) => {
      if (event.type !== "keydown" || !event.ctrlKey || !event.shiftKey) return true;
      if (event.code === "KeyC") {
        copy();
        return false;
      }
      if (event.code === "KeyV") {
        event.preventDefault();
        paste();
        return false;
      }
      return true;
    });
    const onContextMenu = (event: MouseEvent) => {
      event.preventDefault();
      paste();
    };
    element.addEventListener("mouseup", copy);
    element.addEventListener("contextmenu", onContextMenu);
    const observer = new ResizeObserver(() => {
      try {
        fit.fit();
      } catch {
        // The box is hidden or has no size.
      }
    });
    observer.observe(element);

    return () => {
      over = true;
      observer.disconnect();
      element.removeEventListener("mouseup", copy);
      element.removeEventListener("contextmenu", onContextMenu);
      for (const subscription of subscriptions) subscription.dispose();
      socket.close();
      pane.current = null;
      term.dispose();
    };
  }, [path, attempt, autoFocus]);

  // The sheet this pane is shown in came back for the keyboard, as it did when the shell first opened.
  useEffect(() => {
    if (focused) pane.current?.focus();
  }, [focused]);

  // The person changed the theme: give the pane its colours again, without reopening the shell.
  // biome-ignore lint/correctness/useExhaustiveDependencies: `theme` is read from the document by `themeOf`, not from this scope
  useEffect(() => {
    const term = pane.current;
    if (term) term.options.theme = themeOf();
  }, [theme]);

  return (
    <div className={cn("flex min-h-0 flex-col gap-3", className)}>
      <div
        role={status === "ended" ? (opened ? "status" : "alert") : "status"}
        data-testid={status === "ended" ? "terminal-ended" : undefined}
        className="flex min-h-10 flex-wrap items-center gap-3"
      >
        {status === "ended" ? (
          <>
            <span
              className={cn(
                "min-w-0 flex-1 break-words text-sm",
                opened ? "text-secondary" : "text-error-text",
              )}
            >
              {endMessage(opened)}
            </span>
            <Button size="sm" variant="secondary" onClick={() => setAttempt((n) => n + 1)}>
              Open again
            </Button>
          </>
        ) : (
          <>
            <span className="flex items-center gap-2">
              <span
                aria-hidden="true"
                className={cn(
                  "size-2 rounded-full",
                  status === "connecting" ? "bg-progress" : "bg-complete",
                )}
              />
              <span
                className={cn(
                  "text-sm font-semibold",
                  status === "connecting" ? "text-progress-text" : "text-complete-text",
                )}
              >
                {status === "connecting" ? "Connecting" : "Ready"}
              </span>
            </span>
            <span className="ml-auto text-xs text-secondary">
              Ctrl+Shift+C copies a selection, Ctrl+Shift+V pastes
            </span>
          </>
        )}
      </div>
      <div
        ref={host}
        data-testid="terminal"
        data-status={status}
        className="min-h-0 flex-1 overflow-hidden rounded-card bg-terminal p-3"
      />
    </div>
  );
}
