/**
 * The island a message is written in: a field that grows with the text (Enter sends, Shift+Enter makes a new line, and
 * an Enter that confirms an input-method candidate does neither), the choices on its left and Send on its right, with
 * the page's further controls beside Send. The choices, the notice and what Send does are the page's; this holds the
 * field, at the size the rest of the chat is read at. Send is only ever pressable when there is something to send; when it is not, the reason waits in its
 * tooltip.
 */
import { ArrowElbowDownLeftIcon, PaperPlaneRightIcon } from "@phosphor-icons/react";
import { type ReactNode, useEffect, useLayoutEffect, useRef } from "react";
import { ChatBoxBase } from "../ui/chat/ChatBoxBase";
import { Button } from "../ui/components/Button";
import { cn } from "../ui/lib/cn";
import { isImeConfirmation } from "../ui/lib/imeComposition";
import { TextArea } from "../ui/page/Field";
import { Notice } from "../ui/page/Notice";

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
   * One row, with Send as an icon beside the field, for the narrow sheet of the Moebius agent. The field is the thing being read
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

  const editor = (
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
      className={cn(
        "max-h-60 min-h-8 resize-none",
        // In the compact field the box around it is the field: no edge, no colour and no focus outline of its own.
        compact &&
          "min-h-7 flex-1 bg-transparent! px-0 py-1 shadow-none! focus-visible:outline-none",
      )}
    />
  );

  if (compact) {
    // A quiet field with the edge of a field, one row, and a small chip inside it that sends. The focus is said by the
    // edge of the whole field thickening to a second hairline, not by a ring round it.
    return (
      <div className="flex w-full flex-col gap-2">
        {error && <Notice tone="error">{error}</Notice>}
        <div className="flex items-end gap-2 rounded-card border border-hairline-strong bg-panel py-1.5 pr-1.5 pl-3 transition-colors duration-(--motion-routine) ease-out has-[textarea:focus-visible]:shadow-[0_0_0_1px_var(--hairline-strong)]">
          {editor}
          {actions}
          <Button
            variant="icon"
            size="sm"
            onClick={onSend}
            disabled={!canSend}
            aria-label={sendLabel}
            title={canSend ? sendLabel : sendHint}
            className={cn(
              "shrink-0 bg-hairline-strong!",
              canSend && "bg-accent-fill! text-white hover:text-white",
            )}
          >
            <ArrowElbowDownLeftIcon aria-hidden="true" className="size-4" />
          </Button>
        </div>
      </div>
    );
  }

  return (
    <ChatBoxBase
      inline={false}
      error={error}
      pills={pills}
      editor={editor}
      footerRight={
        <>
          {actions}
          <Button
            size="md"
            onClick={onSend}
            disabled={!canSend}
            // A control that simply has nothing to do says nothing; a reason is for what the page knows and the
            // person cannot see — that a repository has to be chosen first, say.
            title={canSend ? sendTitle : sendHint}
          >
            <PaperPlaneRightIcon aria-hidden="true" className="size-4" weight="fill" />
            {sendLabel}
          </Button>
        </>
      }
    />
  );
}
