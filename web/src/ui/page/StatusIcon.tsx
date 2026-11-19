/**
 * The small marks that replace coloured pills for states: `StatusIcon` for the state of a column, `PriorityBars` for
 * priority, `RunRing` for a run that is going and `LabelDot` for a label. Each says its state by its shape as well as
 * its hue, carries an accessible name (or, with `decorative`, hides itself because a word stands beside it), and takes
 * its colour from the status tokens of `styles/theme.css`, so both themes work. They replace the coloured pills, of which none is left;
 * the board, the Inbox and Fleet use them through `MarkLabel.tsx`, the other pages through `StateLabel.tsx`.
 *
 * ```tsx
 * <StatusIcon state="inReview" />        // announced as "In review"
 * <PriorityBars priority="urgent" />     // the only coloured priority
 * <RunRing />                            // a turning ring, announced as "Running"
 * <LabelDot name="Design" />             // a dot and the word
 * ```
 */
import type { ReactNode } from "react";
import { cn } from "../lib/cn";

/** The states of a column, in the order a card passes through them. */
export const STATUS_STATES = [
  "backlog",
  "notStarted",
  "inProgress",
  "inReview",
  "readyToMerge",
  "done",
  "blocked",
] as const;
export type StatusState = (typeof STATUS_STATES)[number];

/** What each state is called to a person hearing it. */
export const STATUS_NAMES: Record<StatusState, string> = {
  backlog: "Backlog",
  notStarted: "Not started",
  inProgress: "In progress",
  inReview: "In review",
  readyToMerge: "Ready to merge",
  done: "Done",
  blocked: "Blocked",
};

const STATUS_HUE: Record<StatusState, string> = {
  backlog: "var(--status-todo)",
  notStarted: "var(--status-todo)",
  inProgress: "var(--status-progress)",
  inReview: "var(--status-review)",
  readyToMerge: "var(--status-done)",
  done: "var(--status-done)",
  blocked: "var(--status-error)",
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

/** A thick arc inside the ring: radius 3 with a 6 wide stroke fills a pie of the given fraction. */
function Pie({ fraction, hue }: { fraction: number; hue: string }) {
  const c = 2 * Math.PI * 3;
  return (
    <circle
      cx="8"
      cy="8"
      r="3"
      stroke={hue}
      strokeWidth="6"
      strokeDasharray={`${c * fraction} ${c}`}
      transform="rotate(-90 8 8)"
    />
  );
}

function body(state: StatusState, hue: string): ReactNode {
  const ring = <circle cx="8" cy="8" r="6" stroke={hue} strokeWidth="1.5" />;
  switch (state) {
    case "backlog":
      return <circle cx="8" cy="8" r="6" stroke={hue} strokeWidth="1.5" strokeDasharray="2 2" />;
    case "notStarted":
      return ring;
    case "inProgress":
      return (
        <>
          {ring}
          <Pie fraction={0.5} hue={hue} />
        </>
      );
    case "inReview":
      return (
        <>
          {ring}
          <Pie fraction={0.75} hue={hue} />
        </>
      );
    case "readyToMerge":
      return (
        <>
          {ring}
          <path
            d="M6.5 5.5 9.5 8 6.5 10.5"
            stroke={hue}
            strokeWidth="1.5"
            strokeLinecap="round"
            strokeLinejoin="round"
          />
        </>
      );
    case "done":
      return (
        <>
          <circle cx="8" cy="8" r="7" fill={hue} />
          <path
            d="M5 8.2 7.1 10.2 11 6"
            stroke="var(--background)"
            strokeWidth="1.5"
            strokeLinecap="round"
            strokeLinejoin="round"
          />
        </>
      );
    case "blocked":
      return (
        <>
          <circle cx="8" cy="8" r="7" fill={hue} />
          <path d="M5 8h6" stroke="var(--background)" strokeWidth="1.75" strokeLinecap="round" />
        </>
      );
  }
}

/** The state of a column as a 16px icon: dashed ring, ring, half, three quarters, chevron, check, bar. */
export function StatusIcon({ state, decorative, className }: MarkProps & { state: StatusState }) {
  return (
    <Mark
      name={STATUS_NAMES[state]}
      decorative={decorative}
      className={className}
      testId={`status-${state}`}
    >
      {body(state, STATUS_HUE[state])}
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

/**
 * A run that is going: a ring in the accent with a gap, turning. Reduced motion stops the turning and the arc still
 * says it is a run.
 */
export function RunRing({ decorative, className }: MarkProps) {
  return (
    <Mark
      name="Running"
      decorative={decorative}
      testId="run-ring"
      className={cn("animate-spin motion-reduce:animate-none", className)}
    >
      <circle cx="8" cy="8" r="6" stroke="var(--hairline-strong)" strokeWidth="1.5" />
      <path d="M8 2a6 6 0 0 1 6 6" stroke="var(--accent)" strokeWidth="1.5" strokeLinecap="round" />
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
