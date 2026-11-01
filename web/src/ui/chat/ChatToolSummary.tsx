// From BloopAI/vibe-kanban packages/ui/src/components/ChatToolSummary.tsx (Apache-2.0), changed: React 19 ref prop in place of forwardRef; the icon is picked from the tool's name (read, edit, write, bash, search, web) instead of Vibe Kanban's action types; the pill can carry a tag (Blocked, Running) and a note (how long it ran); no references to other components of Vibe Kanban.
import {
  FileTextIcon,
  GlobeIcon,
  ListMagnifyingGlassIcon,
  NotePencilIcon,
  TerminalWindowIcon,
} from "@phosphor-icons/react";
import type { ReactNode, Ref } from "react";
import { cn } from "../lib/cn";
import { ToolStatusDot, type ToolStatusLike } from "./ToolStatusDot";

interface ChatToolSummaryProps {
  summary: string;
  className?: string;
  expanded?: boolean;
  onToggle?: () => void;
  status?: ToolStatusLike;
  /** The name of the tool (`read`, `bash`, an MCP tool's `server · tool`), which picks the icon. */
  toolName?: string;
  /** Whether the summary is cut, so that a click can show all of it. */
  isTruncated?: boolean;
  /** Something on the right of the summary: a tag, how long the tool ran. */
  trailing?: ReactNode;
  ref?: Ref<HTMLSpanElement>;
}

function iconOf(toolName: string | undefined) {
  switch (toolName?.toLowerCase()) {
    case "bash":
      return TerminalWindowIcon;
    case "read":
      return FileTextIcon;
    case "edit":
    case "write":
      return NotePencilIcon;
    case "webfetch":
    case "websearch":
      return GlobeIcon;
    default:
      return ListMagnifyingGlassIcon;
  }
}

export function ChatToolSummary({
  summary,
  className,
  expanded,
  onToggle,
  status,
  toolName,
  isTruncated,
  trailing,
  ref,
}: ChatToolSummaryProps) {
  // Can expand if text is truncated and onToggle is provided
  const canExpand = Boolean(isTruncated && onToggle);
  const Icon = iconOf(toolName);

  const Root = canExpand ? "button" : "div";

  return (
    <Root
      type={canExpand ? "button" : undefined}
      className={cn(
        "flex w-full items-center gap-base text-left text-sm text-low",
        canExpand && "cursor-pointer",
        className,
      )}
      onClick={canExpand ? onToggle : undefined}
      aria-expanded={canExpand ? expanded : undefined}
    >
      <span className="relative shrink-0 pt-0.5">
        <Icon className="size-icon-base" />
        {status && <ToolStatusDot status={status} className="absolute -bottom-0.5 -left-0.5" />}
      </span>
      {toolName && (
        <span className="shrink-0 rounded-sm bg-card px-base py-half font-mono text-xs text-high">
          {toolName}
        </span>
      )}
      <span
        ref={ref}
        className={cn(
          "min-w-0 flex-1 font-mono text-xs",
          !expanded && "truncate",
          expanded && "whitespace-pre-wrap break-all",
        )}
      >
        {summary}
      </span>
      {trailing}
    </Root>
  );
}
