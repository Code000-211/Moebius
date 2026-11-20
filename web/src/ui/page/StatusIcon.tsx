/**
 * The small marks that replace coloured pills for states: `StatusIcon` for the state of a column, `PriorityBars` for
 * priority and `LabelDot` for a label. Each says its state by its shape as well as
 * its hue, carries an accessible name (or, with `decorative`, hides itself because a word stands beside it), and takes
 * its colour from the status tokens of `styles/theme.css`, so both themes work. They replace the coloured pills, of which none is left;
 * the board, the Inbox and Fleet use them through `MarkLabel.tsx`, the other pages through `StateLabel.tsx`.
 *
 * ```tsx
 * <StatusIcon state="inReview" />        // announced as "In review"
 * <PriorityBars priority="urgent" />     // the only coloured priority
 * <StatusIcon state="running" />         // a still ring with a quarter arc, announced as "Running"
 * <LabelDot name="Design" />             // a dot and the word
 * ```
 */
import type { ReactNode } from "react";
import { cn } from "../lib/cn";

/** The five status icons, Moebius's own: each is told by its shape as well as its colour. */
export const STATUS_KINDS = ["needsYou", "running", "inReview", "done", "notStarted"] as const;
export type StatusKind = (typeof STATUS_KINDS)[number];

/** What each icon is called to a person hearing it. */
export const STATUS_KIND_NAMES: Record<StatusKind, string> = {
  needsYou: "Needs you",
  running: "Running",
  inReview: "In review",
  done: "Done",
  notStarted: "Not started",
};

/**
 * The states callers name, each mapped to one of the five icons. A column, a pull request or a tool call that is not
 * listed here is "not started".
 */
export const STATUS_STATES = [
  "backlog",
  "notStarted",
  "inProgress",
  "inReview",
  "readyToMerge",
  "done",
  "blocked",
  "needsYou",
  "running",
] as const;
export type StatusState = (typeof STATUS_STATES)[number];

export const STATUS_OF: Record<StatusState, StatusKind> = {
  backlog: "notStarted",
  notStarted: "notStarted",
  inProgress: "running",
  running: "running",
  inReview: "inReview",
  readyToMerge: "inReview",
  done: "done",
  blocked: "needsYou",
  needsYou: "needsYou",
};

/** The icon of a state a caller names; a name that is not a state is "not started". */
export function statusKind(state: string): StatusKind {
  return (STATUS_OF as Record<string, StatusKind | undefined>)[state] ?? "notStarted";
}

const STATUS_HUE: Record<StatusKind, string> = {
  needsYou: "var(--needs-you)",
  running: "var(--running)",
  inReview: "var(--status-review)",
  done: "var(--status-done)",
  notStarted: "var(--status-todo)",
};

interface MarkProps {
  /** Hide the mark from assistive technology, when a word beside it already names the state. */
  decorative?: boolean;
  className?: string;
}

function Mark({
  name,
  decorative,
  className,
  children,
  testId,
}: MarkProps & { name: string; children: ReactNode; testId?: string }) {
  return (
    <svg
      viewBox="0 0 16 16"
      width={16}
      height={16}
      data-testid={testId}
      className={cn("shrink-0", className)}
      fill="none"
      role={decorative ? undefined : "img"}
      aria-label={decorative ? undefined : name}
      aria-hidden={decorative ? true : undefined}
    >
      {children}
    </svg>
  );
}

function body(kind: StatusKind, hue: string): ReactNode {
  const ring = <circle cx="8" cy="8" r="6.25" stroke={hue} strokeWidth="1.5" />;
  switch (kind) {
    case "needsYou":
      return (
        <>
          {ring}
          <path d="M8 4.75v3.5" stroke={hue} strokeWidth="1.5" strokeLinecap="round" />
          <circle cx="8" cy="10.9" r="0.9" fill={hue} />
        </>
      );
    case "running": {
      const c = 2 * Math.PI * 6.25;
      return (
        <>
          <circle cx="8" cy="8" r="6.25" stroke={hue} strokeWidth="1.5" opacity="0.25" />
          <circle
            cx="8"
            cy="8"
            r="6.25"
            stroke={hue}
            strokeWidth="1.5"
            strokeLinecap="round"
            strokeDasharray={`${c / 4} ${c}`}
            transform="rotate(-90 8 8)"
          />
        </>
      );
    }
    case "inReview":
      return (
        <>
          {ring}
          <path d="M8 4.25a3.75 3.75 0 0 1 0 7.5Z" fill={hue} />
        </>
      );
    case "done":
      return (
        <>
          <circle cx="8" cy="8" r="7" fill={hue} />
          <path
            d="M5 8.2 7.1 10.2 11 6"
            stroke="var(--panel)"
            strokeWidth="1.5"
            strokeLinecap="round"
            strokeLinejoin="round"
          />
        </>
      );
    case "notStarted":
      return (
        <circle cx="8" cy="8" r="6.25" stroke={hue} strokeWidth="1.5" strokeDasharray="2.2 2.2" />
      );
  }
}

/**
 * One of the five status icons at 16px: needs you, running, in review, done, not started. A state a caller names (a
 * column such as "In progress") is mapped to its icon; none of them moves.
 */
export function StatusIcon({ state, decorative, className }: MarkProps & { state: StatusState }) {
  const kind = statusKind(state);
  return (
    <Mark
      name={STATUS_KIND_NAMES[kind]}
      decorative={decorative}
      className={className}
      testId={`status-${kind}`}
    >
      {body(kind, STATUS_HUE[kind])}
    </Mark>
  );
}

/** Priorities, lowest first. */
export const PRIORITIES = ["none", "low", "medium", "high", "urgent"] as const;
export type Priority = (typeof PRIORITIES)[number];

const PRIORITY_NAMES: Record<Priority, string> = {
  none: "No priority",
  low: "Low priority",
  medium: "Medium priority",
  high: "High priority",
  urgent: "Urgent",
};

/**
 * Priority as three signal bars, filled in grey as far as the level goes (none is three dashes). Urgent is the only
 * coloured one, and a different shape too: a square with an exclamation mark.
 */
export function PriorityBars({
  priority,
  decorative,
  className,
}: MarkProps & { priority: Priority }) {
  const name = PRIORITY_NAMES[priority];
  const testId = `priority-${priority}`;
  if (priority === "urgent") {
    return (
      <Mark name={name} decorative={decorative} className={className} testId={testId}>
        <rect x="1.5" y="1.5" width="13" height="13" rx="3" fill="var(--status-error)" />
        <path d="M8 4.5v4.5" stroke="var(--background)" strokeWidth="1.75" strokeLinecap="round" />
        <circle cx="8" cy="11.5" r="1" fill="var(--background)" />
      </Mark>
    );
  }
  const filled = { none: 0, low: 1, medium: 2, high: 3 }[priority];
  return (
    <Mark name={name} decorative={decorative} className={className} testId={testId}>
      {priority === "none" ? (
        <path
          d="M3 8h2.5M6.75 8h2.5M10.5 8H13"
          stroke="var(--text-muted)"
          strokeWidth="1.5"
          strokeLinecap="round"
        />
      ) : (
        [0, 1, 2].map((i) => (
          <rect
            key={i}
            data-bar={i < filled ? "filled" : "empty"}
            x={2.5 + i * 4}
            y={10 - i * 3}
            width="3"
            height={3 + i * 3}
            rx="0.75"
            fill={i < filled ? "var(--text-secondary)" : "var(--text-muted)"}
            opacity={i < filled ? 1 : 0.35}
          />
        ))
      )}
    </Mark>
  );
}

/** A label as a dot in its colour and its word; the word is the name, so the dot is hidden from assistive technology. */
export function LabelDot({
  name,
  color = "var(--text-muted)",
  className,
}: {
  name: string;
  /** Any CSS colour; a label without one gets a grey dot. */
  color?: string;
  className?: string;
}) {
  return (
    <span
      className={cn("inline-flex items-center gap-1.5 text-metadata text-secondary", className)}
    >
      <span
        aria-hidden="true"
        data-testid="label-dot"
        className="size-2 shrink-0 rounded-full"
        style={{ backgroundColor: color }}
      />
      {name}
    </span>
  );
}
