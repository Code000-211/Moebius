/**
 * The island a message is written in: an auto-growing field (Enter sends, Shift+Enter makes a new line, and an Enter
 * that confirms an input-method candidate does neither), the pills on its left and the Send button on its right. The
 * pills, the notice and what Send does are the page's; this holds the field.
 */
import { PaperPlaneRightIcon } from "@phosphor-icons/react";
import { type ReactNode, useEffect, useLayoutEffect, useRef } from "react";
import { ChatBoxBase } from "../ui/chat/ChatBoxBase";
import { Button } from "../ui/components/Button";
import { isImeConfirmation } from "../ui/lib/imeComposition";

/** The most rows the field grows to before it scrolls. */
const MAX_ROWS = 8;

interface ComposerProps {
  value: string;
  onChange: (value: string) => void;
  onSend: () => void;
  /** Whether Send can be pressed (there is something to send, and the chat can take it). */
  canSend: boolean;
  placeholder?: string;
  pills?: ReactNode;
  /** The last thing that went wrong, as a short line above the field. */
  error?: string | null;
  /** Whether the agent works: the island lights up. */
  running?: boolean;
  /** Something between the pills and the buttons on the right, such as a note. */
  extra?: ReactNode;
  autoFocus?: boolean;
}

export function Composer({
  value,
  onChange,
  onSend,
  canSend,
  placeholder = "Write to the agent",
  pills,
  error,
  running,
  extra,
  autoFocus,
}: ComposerProps) {
  const field = useRef<HTMLTextAreaElement>(null);

  // Focuses the field when the composer appears, where the page asks for it.
  useEffect(() => {
    if (autoFocus) field.current?.focus();
  }, [autoFocus]);

  // Grows the field with its text, up to MAX_ROWS lines.
  // biome-ignore lint/correctness/useExhaustiveDependencies: it runs when the text changes
  useLayoutEffect(() => {
    const element = field.current;
    if (!element) return;
    element.style.height = "auto";
    const line = Number.parseFloat(getComputedStyle(element).lineHeight) || 20;
    element.style.height = `${Math.min(element.scrollHeight, line * MAX_ROWS)}px`;
  }, [value]);

  return (
    <ChatBoxBase
      error={error}
      isRunning={running}
      pills={pills}
      editor={
        <textarea
          ref={field}
          value={value}
          rows={1}
          aria-label="Message"
          placeholder={placeholder}
          onChange={(event) => onChange(event.target.value)}
          onKeyDown={(event) => {
            if (event.key !== "Enter" || event.shiftKey || isImeConfirmation(event.nativeEvent)) {
              return;
            }
            event.preventDefault();
            if (canSend) onSend();
          }}
          className="max-h-60 w-full resize-none bg-transparent py-half text-sm text-high outline-none placeholder:text-low"
        />
      }
      footerRight={
        <>
          {extra}
          <Button size="sm" onClick={onSend} disabled={!canSend}>
            <PaperPlaneRightIcon className="size-icon-xs" weight="fill" />
            Send
          </Button>
        </>
      }
    />
  );
}
