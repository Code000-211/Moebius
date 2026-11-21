// From BloopAI/vibe-kanban packages/ui/src/components/ChatToolSummary.tsx (Apache-2.0), changed: the row says what the
// tool did in a few plain words with a small icon of its own ("Read wiki.rs", "Ran the tests") instead of the raw
// command; the command is in the box the row opens to. The row is 28px tall and hangs on the thin line of the steps;
// its state is a word with a hue.

import type { StepWords } from "../../chat/conversation";
import { cn } from "../lib/cn";
import { StepGlyph, StepText } from "./StepGlyph";
import { ToolStatusDot, type ToolStatusLike } from "./ToolStatusDot";

interface ChatToolSummaryProps {
  /** What the tool did, in words, with the icon that goes with them. */
  words: StepWords;
  /** The whole path or the tool's name, kept for the tooltip. */
  detail?: string;
  className?: string;
  expanded?: boolean;
  onToggle?: () => void;
  status?: ToolStatusLike;
  /** Whether there is more to see, so that a click opens it. */
  isTruncated?: boolean;
}

export function ChatToolSummary({
  words,
  detail,
  className,
  expanded,
  onToggle,
  status,
  isTruncated,
}: ChatToolSummaryProps) {
  const canExpand = Boolean(isTruncated && onToggle);
  const Root = canExpand ? "button" : "div";

  return (
    <Root
      type={canExpand ? "button" : undefined}
      className={cn(
        "flex min-h-5 w-full items-center gap-2.5 text-left text-muted transition-colors duration-(--motion-routine) ease-out",
        canExpand && "cursor-pointer hover:text-secondary",
        className,
      )}
      onClick={canExpand ? onToggle : undefined}
      aria-expanded={canExpand ? expanded : undefined}
      title={detail || undefined}
    >
      <StepGlyph icon={words.icon} />
      <StepText text={words.text} code={words.code} />
      {status && <ToolStatusDot status={status} />}
    </Root>
  );
}
