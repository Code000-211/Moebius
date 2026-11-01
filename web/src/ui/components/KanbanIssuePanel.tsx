// From BloopAI/vibe-kanban packages/ui/src/components/KanbanIssuePanel.tsx (Apache-2.0), changed: the panel is a read-only inspector, not the form for creating and editing an issue (the title, description editor, tags, assignees and attachments are gone); it keeps the frame (the header with the number, the copy-link and close buttons, Escape to close, the scrolling body with bordered sections) and takes its sections as children; the title wraps however long it is.
import { LinkIcon, XIcon } from "@phosphor-icons/react";
import type { KeyboardEvent, ReactNode } from "react";
import { cn } from "../lib/cn";

export interface KanbanIssuePanelProps {
  /** The card's number as it is written, such as `#12`. */
  displayId: string;
  /** Where the card's issue is, when the app knows: the number is then a link. */
  issueUrl?: string | null;
  title: string;
  /** What sits beside the number, such as the card's status. */
  status?: ReactNode;
  onClose: () => void;
  /** Copies the address of the card; the button shows only with it. */
  onCopyLink?: () => void;
  className?: string;
  /** The sections, each in a `KanbanIssueSection`. */
  children: ReactNode;
}

export function KanbanIssuePanel({
  displayId,
  issueUrl,
  title,
  status,
  onClose,
  onCopyLink,
  className,
  children,
}: KanbanIssuePanelProps) {
  const handleKeyDown = (e: KeyboardEvent) => {
    if (e.key !== "Escape") return;
    const target = e.target as HTMLElement;
    const isEditable =
      target.tagName === "INPUT" || target.tagName === "TEXTAREA" || target.isContentEditable;
    if (isEditable) {
      target.blur();
      e.stopPropagation();
    } else {
      onClose();
    }
  };

  const breadcrumbTextClass =
    "min-w-0 text-sm text-normal truncate rounded-sm px-1 py-0.5 hover:bg-panel hover:text-high transition-colors";

  return (
    <section
      aria-label="Card inspector"
      className={cn(
        "flex h-full min-h-0 flex-col overflow-hidden rounded-xl border border-accent-border bg-surface outline-none",
        className,
      )}
      onKeyDown={handleKeyDown}
      tabIndex={-1}
    >
      {/* Header */}
      <div className="flex shrink-0 items-center justify-between gap-base border-b px-base py-half">
        <div className="flex min-w-0 items-center gap-half font-mono">
          {issueUrl ? (
            <a
              href={issueUrl}
              target="_blank"
              rel="noopener noreferrer"
              className={`${breadcrumbTextClass} shrink-0 text-accent`}
            >
              Card {displayId}
            </a>
          ) : (
            <span className={`${breadcrumbTextClass} shrink-0 text-accent`}>Card {displayId}</span>
          )}
          {onCopyLink && (
            <button
              type="button"
              onClick={onCopyLink}
              className="rounded-sm p-half text-low transition-colors hover:bg-panel hover:text-normal"
              aria-label="Copy the link to this card"
            >
              <LinkIcon className="size-icon-sm" weight="bold" />
            </button>
          )}
        </div>
        <div className="flex items-center gap-base">
          {status}
          <button
            type="button"
            onClick={onClose}
            className="rounded-sm p-half text-low transition-colors hover:bg-panel hover:text-normal"
            aria-label="Close the inspector"
          >
            <XIcon className="size-icon-sm" weight="bold" />
          </button>
        </div>
      </div>

      {/* Scrollable content: the title wraps, however long */}
      <div className="min-h-0 flex-1 overflow-y-auto">
        <h2 className="m-0 px-base py-base text-[15px] font-bold text-high [overflow-wrap:anywhere]">
          {title}
        </h2>
        {children}
      </div>
    </section>
  );
}

/** A section of the panel: a heading in the code font and its content, under a rule. */
export function KanbanIssueSection({
  title,
  children,
  className,
}: {
  title: string;
  children: ReactNode;
  className?: string;
}) {
  return (
    <section aria-label={title} className={cn("border-t px-base py-base", className)}>
      <h3 className="m-0 mb-half font-mono text-[11px] uppercase tracking-wide text-low">
        {title}
      </h3>
      {children}
    </section>
  );
}
