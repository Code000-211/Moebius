/**
 * The island a message is written in: a field that grows with the text (Enter sends, Shift+Enter makes a new line, and
 * an Enter that confirms an input-method candidate does neither), the choices on its left and Send on its right, with
 * the page's further controls beside Send. The choices, the notice and what Send does are the page's; this holds the
 * field, at the size the rest of the chat is read at. Send is only ever pressable when there is something to send; when it is not, the reason waits in its
 * tooltip.
 */
import { PaperPlaneRightIcon } from "@phosphor-icons/react";
import { type ReactNode, useEffect, useLayoutEffect, useRef } from "react";
import { ChatBoxBase } from "../ui/chat/ChatBoxBase";
import { Button } from "../ui/components/Button";
import { isImeConfirmation } from "../ui/lib/imeComposition";
import { TextArea } from "../ui/page/Field";

/** The most rows the field grows to before it scrolls. */
const MAX_ROWS = 8;

interface ComposerProps {
  value: string;
  onChange: (value: string) => void;
  onSend: () => void;
  /** Whether Send can be pressed (there is something to send, and the chat can take it). */
  canSend: boolean;
  /** Why Send cannot be pressed, for the person to read when they ask it. */
  sendHint?: string;
  /**
   * The word on the button. It is "Send" while the chat can be written to, and the page changes it when the button
   * does something else — "Continue" for a chat that has ended, whose message starts a new chat told what was said.
   */
  sendLabel?: string;
  /** What the button is about to do, for the person to read when they ask it. */
  sendTitle?: string;
  placeholder?: string;
  pills?: ReactNode;
  /** The controls left of Send: the chip that says the chat's state and opens its actions. */
  actions?: ReactNode;
  /** The last thing that went wrong, as a short line above the field. */
  error?: string | null;
  autoFocus?: boolean;
  /**
   * One row, with Send as an icon beside the field, for the narrow floating chat. The field is the thing being read
   * and written; the word on the button repeats the placeholder above it.
   */
  compact?: boolean;
}

export function Composer({
  value,
  onChange,
  onSend,
  canSend,
  sendHint,
  sendLabel = "Send",
  sendTitle,
  placeholder = "Write to the agent",
  pills,
  actions,
  error,
  autoFocus,
  compact = false,
}: ComposerProps) {
  const field = useRef<HTMLTextAreaElement>(null);

  // Focuses the field when the composer appears, where the page asks for it.
  useEffect(() => {
    if (autoFocus) field.current?.focus();
  }, [autoFocus]);

  // Grows the field with its text, up to MAX_ROWS lines. The shared `TextArea` is only as tall as its content wants to
  // be; this is the cap that keeps a pasted wall of text from pushing the rest of the chat away.
  // biome-ignore lint/correctness/useExhaustiveDependencies: it runs when the text changes
  useLayoutEffect(() => {
    const element = field.current;
    if (!element) return;
    element.style.height = "auto";
    const line = Number.parseFloat(getComputedStyle(element).lineHeight) || 24;
    element.style.height = `${Math.min(element.scrollHeight, line * MAX_ROWS)}px`;
  }, [value]);

  return (
    <ChatBoxBase
      inline={compact}
      error={error}
      pills={pills}
      editor={
        <TextArea
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
          className="max-h-60 min-h-10 resize-none"
        />
      }
      footerRight={
        <>
          {actions}
          <Button
            size={compact ? "icon" : "md"}
            onClick={onSend}
            disabled={!canSend}
            aria-label={compact ? sendLabel : undefined}
            // A control that simply has nothing to do says nothing; a reason is for what the page knows and the
            // person cannot see — that a repository has to be chosen first, say.
            title={canSend ? (compact ? sendLabel : sendTitle) : sendHint}
          >
            <PaperPlaneRightIcon aria-hidden="true" className="size-4" weight="fill" />
            {compact ? null : sendLabel}
          </Button>
        </>
      }
    />
  );
}
