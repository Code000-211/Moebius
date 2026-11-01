// From BloopAI/vibe-kanban packages/ui/src/components/ChatToolSummary.tsx (Apache-2.0), changed: the icon is picked
// from the tool's name (read, edit, write, bash, search, web) instead of Vibe Kanban's action types; the row is 40px
// tall, the tool is named in a label rather than small monospace, a file is named by its name with its whole path kept
// as the tooltip, what it was given is one line whose end is kept in view and whose beginning fades when the line runs
// out, and its state is a word with a hue.
import {
  FileTextIcon,
  GlobeIcon,
  ListMagnifyingGlassIcon,
  NotePencilIcon,
  TerminalWindowIcon,
} from "@phosphor-icons/react";
import { cn } from "../lib/cn";
import { FadedLine } from "../page/FadedLine";
import { Tag } from "./Tag";
import { ToolStatusDot, type ToolStatusLike } from "./ToolStatusDot";

interface ChatToolSummaryProps {
  /** What the tool was given: the file it read, the command it ran. */
  summary: string;
  /** The whole path behind a summary that names only a file, kept for the tooltip. */
  detail?: string;
  className?: string;
  expanded?: boolean;
  onToggle?: () => void;
  status?: ToolStatusLike;
  /** The name of the tool (`read`, `bash`, an MCP tool's `server · tool`), which picks the icon. */
  toolName?: string;
  /** Whether there is more to see, so that a click opens it. */
  isTruncated?: boolean;
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
  detail,
  className,
  expanded,
  onToggle,
  status,
  toolName,
  isTruncated,
}: ChatToolSummaryProps) {
  const canExpand = Boolean(isTruncated && onToggle);
  const Icon = iconOf(toolName);

  const Root = canExpand ? "button" : "div";

  return (
    <Root
      type={canExpand ? "button" : undefined}
      className={cn(
        "flex min-h-10 w-full flex-wrap items-center gap-3 rounded-control px-3 py-1 text-left transition-colors duration-(--motion-routine) ease-out",
        canExpand && "cursor-pointer hover:bg-hover",
        className,
      )}
      onClick={canExpand ? onToggle : undefined}
      aria-expanded={canExpand ? expanded : undefined}
      title={detail || undefined}
    >
      <Icon aria-hidden="true" className="size-4 shrink-0 text-secondary" />
      {toolName && <Tag className="shrink-0">{toolName}</Tag>}
      <FadedLine text={summary} className="flex-1 font-mono text-sm text-text" />
      {status && <ToolStatusDot status={status} />}
    </Root>
  );
}
