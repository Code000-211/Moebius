/**
 * The "Moebius" button of the page header: opens the agent's panel beside the page and closes it again. It is quiet
 * (the secondary button) and, while the panel is open, pressed: an indigo tint with indigo text and
 * `aria-pressed="true"`. When the panel closes, the focus comes back to it.
 */
import { useEffect, useRef } from "react";
import { Button } from "../../ui/components/Button";
import { cn } from "../../ui/lib/cn";
import { useAgent } from "./AgentContext";

/** Lucide's "sparkles" (ISC), drawn at 16px in the text's colour. */
function SparklesIcon() {
  return (
    <svg
      aria-hidden="true"
      viewBox="0 0 24 24"
      className="size-4"
      fill="none"
      stroke="currentColor"
      strokeWidth="2"
      strokeLinecap="round"
      strokeLinejoin="round"
    >
      <path d="M9.937 15.5A2 2 0 0 0 8.5 14.063l-6.135-1.582a.5.5 0 0 1 0-.962L8.5 9.936A2 2 0 0 0 9.937 8.5l1.582-6.135a.5.5 0 0 1 .963 0L14.063 8.5A2 2 0 0 0 15.5 9.937l6.135 1.581a.5.5 0 0 1 0 .964L15.5 14.063a2 2 0 0 0-1.437 1.437l-1.582 6.135a.5.5 0 0 1-.963 0z" />
      <path d="M20 3v4" />
      <path d="M22 5h-4" />
      <path d="M4 17v2" />
      <path d="M5 18H3" />
    </svg>
  );
}

export function AgentButton() {
  const { open, toggle } = useAgent();
  const button = useRef<HTMLButtonElement>(null);
  const wasOpen = useRef(open);
  useEffect(() => {
    if (wasOpen.current && !open) button.current?.focus();
    wasOpen.current = open;
  }, [open]);
  return (
    <Button
      ref={button}
      type="button"
      variant="secondary"
      size="sm"
      aria-pressed={open}
      onClick={toggle}
      className={cn(open && "bg-accent-tint text-accent-text hover:bg-accent-tint")}
    >
      <SparklesIcon />
      Moebius
    </Button>
  );
}
