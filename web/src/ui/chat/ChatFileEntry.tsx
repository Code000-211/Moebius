// From BloopAI/vibe-kanban packages/ui/src/components/ChatFileEntry.tsx (Apache-2.0), changed: one layout for both
// cases (the original repeated the header); the diff is given as `children`, not as a diff input and a renderer; no
// VSCode, no "open in changes" and no i18n; the header is a real button 40px tall; the file is named by its name, with
// the whole path kept as a tooltip — a row of the same folder over and over says nothing — its state is a word with a
// hue, and the counts of changed lines are figures that do not shift.
import { CaretDownIcon, FileIcon as DefaultFileIcon } from "@phosphor-icons/react";
import type { ElementType, ReactNode } from "react";
import { cn } from "../lib/cn";
import { fileName, plural } from "../lib/format";
import { FadedLine } from "../page/FadedLine";
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

  return (
    <div className={cn("overflow-hidden rounded-control bg-sunken", className)}>
      <button
        type="button"
        className="flex min-h-8 w-full flex-wrap items-center gap-3 rounded-control px-3 text-left transition-colors duration-(--motion-routine) ease-out hover:bg-hover"
        onClick={onToggle}
        aria-expanded={onToggle ? expanded : undefined}
      >
        <FileIcon aria-hidden="true" className="size-4 shrink-0 text-secondary" />
        <FadedLine
          text={fileName(filename)}
          title={filename}
          className="flex-1 font-mono text-sm text-text"
        />
        {hasStats && (
          <span className="shrink-0 text-sm tabular-nums">
            {additions !== undefined && additions > 0 && (
              <span
                className="text-diff-added-mark"
                title={plural(additions, "line added", "lines added")}
              >
                +{additions}
              </span>
            )}
            {additions !== undefined && deletions !== undefined && " "}
            {deletions !== undefined && deletions > 0 && (
              <span
                className="text-diff-removed-mark"
                title={plural(deletions, "line removed", "lines removed")}
              >
                -{deletions}
              </span>
            )}
          </span>
        )}
        {status && <ToolStatusDot status={status} />}
        {onToggle && (
          <CaretDownIcon
            aria-hidden="true"
            className={cn(
              "size-4 shrink-0 text-secondary transition-transform duration-(--motion-routine) ease-out",
              !expanded && "-rotate-90",
            )}
          />
        )}
      </button>
      {expanded && children}
    </div>
  );
}
