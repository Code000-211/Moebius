/**
 * What a page draws while the words it will show are still being read.
 *
 * A placeholder takes the shape of the thing that will replace it — the rows of a list, the lanes of a board, the lines
 * of a file — so the page fills in place rather than moving under the eyes, and the shape already says what is coming.
 * A spinner in the middle of an empty space says only that something is happening, and the space changes size when the
 * answer arrives. `docs/design-web.md` asks for a placeholder that matches the expected layout; these are that.
 *
 * They are for the first read only. A refresh keeps what is already on the screen, so a page shows these while its
 * query is pending and never while it is fetching again something a person is in the middle of reading.
 *
 * The bars carry no words. A sentence that is about to be replaced by a different sentence is a promise nothing can
 * keep, and the shape of it is the same promise kept quietly. The one thing still worth saying out loud is that the
 * page is being read, so a placeholder is a `role="status"` region whose message is spoken rather than drawn — the same
 * message the spinner used to show. The region is drawn empty and given its message right after it is mounted, because
 * a live region is announced when its text changes and not reliably when it is inserted already holding text. The
 * bars themselves are hidden from a screen reader: they are the shape of writing, not writing.
 *
 * The fill is the hairline token, the tenth of the text colour that a divider is drawn with, so a bar reads as a line
 * that has not been drawn yet rather than as a control that cannot be used. It works in either theme and adds no colour
 * of its own to the standard.
 */
import { type ReactNode, useEffect, useState } from "react";
import { cn } from "../lib/cn";

/** The widths the lines of one block are drawn at, so several bars do not read as one long word. */
const WIDTHS = ["w-full", "w-3/4", "w-2/3", "w-1/2"];

/** One line of text that is not there yet. */
export function Skeleton({ className }: { className?: string }) {
  return (
    <span
      aria-hidden="true"
      className={cn(
        "block h-3 shrink-0 rounded-round bg-hairline animate-pulse motion-reduce:animate-none",
        className,
      )}
    />
  );
}

/**
 * The region a placeholder is announced from, so the state the spinner used to draw is still said. A shape that names
 * what it is reading wraps itself in one; a page that fills two places at once wraps both in a single `Reading`, so
 * the state is said once rather than once per column.
 */
export function Reading({
  message,
  children,
  className,
}: {
  /** What is being read, the way the page has always said it: "Reading the board". */
  message?: string;
  children: ReactNode;
  className?: string;
}) {
  // A live region is announced when its text changes, not when it is inserted holding text, so it is drawn empty and
  // given its words once it is on the page.
  const [said, setSaid] = useState("");
  useEffect(() => setSaid(message ?? ""), [message]);
  return (
    <div role={message ? "status" : undefined} className={className}>
      {message && <span className="sr-only">{said}</span>}
      {children}
    </div>
  );
}

/** `lines` bars, each shorter than the one before until the block runs out of room. */
function Lines({ lines, className }: { lines: number; className?: string }) {
  return (
    <>
      {Array.from({ length: lines }, (_, place) => (
        <Skeleton
          // biome-ignore lint/suspicious/noArrayIndexKey: the bars of a placeholder draw the same every time, in order
          key={place}
          className={cn(WIDTHS[place % WIDTHS.length], className)}
        />
      ))}
    </>
  );
}

/** The rows of the list on the left of a page, on their way: `ListCard`s with nothing said on them yet. */
export function SkeletonList({
  rows = 4,
  message,
  className,
}: {
  rows?: number;
  message?: string;
  className?: string;
}) {
  return (
    <Reading message={message} className={cn("flex flex-col gap-2", className)}>
      {Array.from({ length: rows }, (_, place) => (
        <div
          // biome-ignore lint/suspicious/noArrayIndexKey: the rows of a placeholder draw the same every time, in order
          key={place}
          className="flex flex-col gap-2 rounded-control bg-surface p-3"
        >
          <Skeleton className="h-4 w-2/5" />
          <Skeleton className="w-3/4" />
        </div>
      ))}
    </Reading>
  );
}

/** A box of a page — a heading and the writing under it — that has not been filled in yet. */
export function SkeletonCard({
  lines = 3,
  message,
  className,
}: {
  lines?: number;
  message?: string;
  className?: string;
}) {
  return (
    <Reading message={message} className={className}>
      <div className="flex flex-col gap-3 rounded-card bg-surface p-4 shadow-card">
        <Skeleton className="h-4 w-1/4" />
        <Lines lines={lines} />
      </div>
    </Reading>
  );
}

/** A block of writing still on its way: a transcript, the body of a file, the output of a terminal. */
export function SkeletonLines({
  lines = 5,
  message,
  className,
}: {
  lines?: number;
  message?: string;
  className?: string;
}) {
  return (
    <Reading message={message} className={cn("flex flex-col gap-3", className)}>
      <Lines lines={lines} />
    </Reading>
  );
}

/**
 * A conversation on its way: the person's message in its band and the agent's answer as text under it, twice over.
 * The turns of a chat are read from their end, so what arrives has to look like turns at once — a page that fills with
 * rows of a list instead reads as a different page being replaced.
 */
export function SkeletonThread({
  turns = 2,
  message,
  className,
}: {
  turns?: number;
  message?: string;
  className?: string;
}) {
  return (
    <Reading message={message} className={cn("flex flex-col gap-6", className)}>
      {Array.from({ length: turns }, (_, place) => (
        <div
          // biome-ignore lint/suspicious/noArrayIndexKey: the turns of a placeholder draw the same every time, in order
          key={place}
          className="flex min-w-0 flex-col gap-4"
        >
          {/* The person's message, in the band of the sunken colour it is always written in. */}
          <div className="w-full rounded-control bg-sunken">
            <div className="flex min-h-10 items-center gap-3 px-4">
              <Skeleton className="size-4" />
              <Skeleton className="h-4 w-16" />
            </div>
            <div className="flex flex-col gap-2 px-4 pb-4">
              <Skeleton className="w-1/2" />
            </div>
          </div>
          {/* The answer: body text on a readable measure, and no box around it. */}
          <div className="flex max-w-prose flex-col gap-2">
            <Lines lines={3} />
          </div>
        </div>
      ))}
    </Reading>
  );
}

/** The lanes of a board, with the room the cards will take up left empty in each of them. */
export function SkeletonBoard({
  columns = 4,
  message,
  className,
}: {
  columns?: number;
  message?: string;
  className?: string;
}) {
  return (
    <Reading message={message} className={cn("flex gap-4", className)}>
      {Array.from({ length: columns }, (_, lane) => (
        <div
          // biome-ignore lint/suspicious/noArrayIndexKey: the lanes of a placeholder draw the same every time, in order
          key={lane}
          className="flex min-h-40 min-w-0 flex-1 flex-col gap-3 rounded-card bg-lane p-3"
        >
          <Skeleton className="h-4 w-1/3" />
          <div className="flex flex-col gap-3 rounded-card bg-surface p-4 shadow-card">
            <Skeleton className="h-4 w-2/3" />
            <Lines lines={2} />
          </div>
          <div className="flex flex-col gap-3 rounded-card bg-surface p-4 shadow-card">
            <Skeleton className="h-4 w-1/2" />
            <Lines lines={1} />
          </div>
        </div>
      ))}
    </Reading>
  );
}
