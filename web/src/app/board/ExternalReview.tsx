/**
 * The review of a pull request someone else opened, as the card window shows it (ADR 0029): what the Reviewer concluded
 * (approved, or changes requested), its summary, and the comments it wrote on lines of the diff, each in a box that says
 * which file and line it is about, and, for a change whose code is right but whose measurement is missing, what is awaited, with the way to the review on the forge. While the author's `/done` waits for the
 * checks or the Reviewer reads, one quiet line says so.
 */
import { ArrowSquareOutIcon } from "@phosphor-icons/react";
import type { ExternalReview as Round } from "../../api/types/board";
import { StatusIcon } from "../../ui/page/StatusIcon";

/** What a round that has no verdict yet or came to nothing says, or null when it has a verdict to show. */
function pending(review: Round): string | null {
  switch (review.state) {
    case "wanted":
      return "Review starts once the checks have passed";
    case "running":
      return "Reviewing the pull request";
    case "failed":
      return "The last review gave no result";
    default:
      return review.verdict ? null : "The last review was dropped";
  }
}

export function ExternalReview({ review, forge }: { review: Round; forge: string }) {
  const waiting = pending(review);
  if (waiting) {
    return (
      <p data-testid="external-review" className="m-0 text-sm text-secondary">
        {waiting}
      </p>
    );
  }
  const approved = review.verdict === "approve" || review.verdict === "approve_code";
  const needsMeasurement = review.verdict === "approve_code";
  const merged = review.state === "merged";
  return (
    <section
      data-testid="external-review"
      aria-label="Review of the pull request"
      className="flex min-w-0 flex-col gap-2"
    >
      <div className="flex min-w-0 flex-wrap items-center justify-between gap-x-4 gap-y-1">
        <div className="flex min-w-0 flex-wrap items-center gap-x-3 gap-y-1">
          <span className="inline-flex min-h-7 items-center gap-1.5 text-sm font-medium text-text">
            <StatusIcon state={approved ? "done" : "inReview"} decorative />
            {needsMeasurement
              ? "Code approved, measurement needed"
              : approved
                ? merged
                  ? "Approved and merged"
                  : "Approved"
                : "Changes requested"}
          </span>
          <span className="text-sm text-muted">{`Round ${review.round} of ${review.round_limit}`}</span>
          {!review.current && (
            <span className="text-sm text-muted">Read an earlier version of the pull request</span>
          )}
        </div>
        {review.url && (
          <a
            href={review.url}
            target="_blank"
            rel="noopener noreferrer"
            className="inline-flex min-h-7 items-center gap-1 text-sm text-accent-text"
          >
            {`Open on ${forge}`}
            <ArrowSquareOutIcon className="size-3.5" aria-hidden="true" />
          </a>
        )}
      </div>
      {needsMeasurement && (
        <p data-testid="measurement-needed" className="m-0 text-sm text-secondary">
          The code is right, but the measurement the repository asks for is missing. It is not
          merged. A comment that starts with "## Measurement" from a registered measurer starts the
          next round.
        </p>
      )}
      {review.summary && (
        <p className="m-0 whitespace-pre-line text-base text-text [overflow-wrap:anywhere]">
          {review.summary}
        </p>
      )}
      {review.comments.length > 0 && (
        <ul className="m-0 flex list-none flex-col gap-2 p-0" aria-label="Comments on lines">
          {review.comments.map((comment) => (
            <li
              key={`${comment.path}:${comment.line}:${comment.body}`}
              data-testid="review-comment"
              className="flex min-w-0 flex-col gap-1 rounded-panel border border-hairline-strong bg-panel px-3 py-2"
            >
              <span className="font-mono text-sm text-muted [overflow-wrap:anywhere]">
                {`${comment.path} line ${comment.line}`}
              </span>
              <span className="whitespace-pre-line text-base text-text [overflow-wrap:anywhere]">
                {comment.body}
              </span>
            </li>
          ))}
        </ul>
      )}
    </section>
  );
}
