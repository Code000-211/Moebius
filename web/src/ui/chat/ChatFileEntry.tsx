// From BloopAI/vibe-kanban packages/ui/src/components/ChatFileEntry.tsx (Apache-2.0), changed: one layout for both cases (the original repeated the header); the diff is given as `children`, not as a diff input and a renderer; no VSCode, no "open in changes" and no i18n; the header is a real button.
import { CaretDownIcon, FileIcon as DefaultFileIcon } from "@phosphor-icons/react";
import type { ElementType, ReactNode } from "react";
import { cn } from "../lib/cn";
import { ToolStatusDot, type ToolStatusLike } from "./ToolStatusDot";

interface ChatFileEntryProps {
  filename: string;
  additions?: number;
  deletions?: number;
  expanded?: boolean;
  onToggle?: () => void;
  className?: string;
  status?: ToolStatusLike;
  /** Optional file icon override */
  fileIcon?: ElementType;
  /** The diff or the content, shown when expanded. */
  children?: ReactNode;
}

export function ChatFileEntry({
  filename,
  additions,
  deletions,
  expanded = false,
  onToggle,
  className,
  status,
  fileIcon,
  children,
}: ChatFileEntryProps) {
  const hasStats = additions !== undefined || deletions !== undefined;
  const FileIcon = fileIcon ?? DefaultFileIcon;
  const isDenied = status?.status === "denied";

  return (
    <div
      className={cn(
        "overflow-hidden rounded-sm border",
        isDenied && "border-error bg-error/10",
        className,
      )}
    >
      <button
        type="button"
        className={cn(
          "flex w-full items-center p-base text-left",
          isDenied ? "bg-error/20" : "bg-panel",
          !onToggle && "cursor-default",
        )}
        onClick={onToggle}
        aria-expanded={onToggle ? expanded : undefined}
      >
        <span className="flex min-w-0 flex-1 items-center gap-base">
          <span className="relative shrink-0">
            <FileIcon className="size-icon-base" />
            {status && (
              <ToolStatusDot status={status} className="absolute -bottom-0.5 -right-0.5" />
            )}
          </span>
          <span className="truncate font-mono text-xs text-normal">{filename}</span>
          {hasStats && (
            <span className="shrink-0 text-sm">
              {additions !== undefined && additions > 0 && (
                <span className="text-success">+{additions}</span>
              )}
              {additions !== undefined && deletions !== undefined && " "}
              {deletions !== undefined && deletions > 0 && (
                <span className="text-error">-{deletions}</span>
              )}
            </span>
          )}
        </span>
        {onToggle && (
          <CaretDownIcon
            className={cn(
              "size-icon-xs shrink-0 text-low transition-transform",
              !expanded && "-rotate-90",
            )}
          />
        )}
      </button>
      {expanded && children}
    </div>
  );
}
