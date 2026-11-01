/**
 * A terminal of the server in the page: xterm.js on a WebSocket (`protocol.ts` says what goes over it). What a person
 * types goes to the shell as bytes, what the shell prints is written to the screen, and the size in cells goes back as
 * a resize whenever the pane is fitted to its box. Like the desktop app's pane (`widgets/terminal/pane.rs`), a
 * selection is copied when the mouse is let go and with Ctrl+Shift+C, and the clipboard is pasted with Ctrl+Shift+V and
 * with a right click; xterm brackets a paste when the program asked for it. Closing the page or leaving it closes the
 * socket, which kills the shell.
 */
import "@xterm/xterm/css/xterm.css";
import { FitAddon } from "@xterm/addon-fit";
import { WebLinksAddon } from "@xterm/addon-web-links";
import { Terminal as XTerm } from "@xterm/xterm";
import { useEffect, useRef, useState } from "react";
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

/** A colour of the theme (`styles/theme.css`) as the terminal needs it. */
function token(name: string, fallback: string): string {
  const value = getComputedStyle(document.documentElement).getPropertyValue(name).trim();
  return value === "" ? fallback : value;
}

function themeOf() {
  return {
    background: token("--terminal", "#07060a"),
    foreground: token("--terminal-text", "#eae4f2"),
    cursor: token("--accent", "#f27663"),
    selectionBackground: token("--line-strong", "#3b334d"),
  };
}

interface TerminalProps {
  /** The terminal's route, such as `/v1/terminal` or `/v1/runs/3/terminal`. */
  path: string;
  /** Takes the keyboard when the terminal opens. */
  autoFocus?: boolean;
  /** Called once when the shell has ended or could not be opened. */
  onEnded?: (opened: boolean) => void;
  className?: string;
}

export function Terminal({ path, autoFocus, onEnded, className }: TerminalProps) {
  const host = useRef<HTMLDivElement>(null);
  const [status, setStatus] = useState<Status>("connecting");
  const [opened, setOpened] = useState(false);
  const [attempt, setAttempt] = useState(0);
  const ended = useRef(onEnded);
  ended.current = onEnded;

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
      term.dispose();
    };
  }, [path, attempt, autoFocus]);

  return (
    <div className={cn("flex min-h-0 flex-col gap-half", className)}>
      {status === "ended" && (
        <div
          role={opened ? "status" : "alert"}
          data-testid="terminal-ended"
          className="flex items-center gap-base text-xs text-normal"
        >
          <span className={cn("min-w-0 flex-1", !opened && "text-error")}>
            {endMessage(opened)}
          </span>
          <Button size="xs" variant="outline" onClick={() => setAttempt((n) => n + 1)}>
            Open again
          </Button>
        </div>
      )}
      <div
        ref={host}
        data-testid="terminal"
        data-status={status}
        className="min-h-0 flex-1 overflow-hidden rounded-md border border-line bg-[var(--terminal)] p-base"
      />
    </div>
  );
}
