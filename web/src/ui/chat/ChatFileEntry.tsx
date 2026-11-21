// From BloopAI/vibe-kanban packages/ui/src/components/ChatFileEntry.tsx (Apache-2.0), changed: one layout for both
// cases (the original repeated the header); the diff is given as `children`, not as a diff input and a renderer; no
// VSCode, no "open in changes" and no i18n; the header is a real button 32px tall; the file is named by its name, with
// the whole path kept as a tooltip — a row of the same folder over and over says nothing — its state is a word with a
// hue, and the counts of changed lines are figures that do not shift.
import { CaretDownIcon, NotePencilIcon } from "@phosphor-icons/react";
import type { ElementType, ReactNode } from "react";
import { cn } from "../lib/cn";
import { fileName, plural } from "../lib/format";
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
  const FileIcon = fileIcon ?? NotePencilIcon;

  return (
    <div className={cn("overflow-hidden rounded-control", className)}>
      <button
        type="button"
        className="group flex min-h-5 w-full items-center gap-2.5 text-left text-muted transition-colors duration-(--motion-routine) ease-out hover:text-secondary"
        onClick={onToggle}
        aria-expanded={onToggle ? expanded : undefined}
      >
        <FileIcon aria-hidden="true" className="step-mark size-4 shrink-0" />
        <span className="min-w-0 truncate text-sm" title={filename}>
          Edited <span className="ml-0.5 font-mono text-[12px]">{fileName(filename)}</span>
        </span>
        <span className="flex-1" />
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
              "size-4 shrink-0 opacity-0 transition-[opacity,transform] duration-(--motion-routine) ease-out group-hover:opacity-100 group-focus-visible:opacity-100",
              !expanded && "-rotate-90",
            )}
          />
        )}
      </button>
      {expanded && children}
    </div>
  );
}
