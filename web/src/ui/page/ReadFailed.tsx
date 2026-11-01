/**
 * The sentence for a read that failed, with the way to try again.
 *
 * A page falls through to its empty state when it has nothing to show, so a read that failed has to say so first:
 * "No repository yet" is a claim about the server, and the opposite of what happened when the server was not reached.
 */
import { sayError } from "../../api/errorText";
import { Button } from "../components/Button";
import { Notice } from "./Notice";

export function ReadFailed({
  error,
  fallback,
  onRetry,
  busy = false,
  className,
}: {
  error: unknown;
  /** What to say when the failure carries no sentence of its own, such as a network that never reached the server. */
  fallback: string;
  onRetry: () => void;
  /** While the page is reading again, so one press of the button is one attempt. */
  busy?: boolean;
  className?: string;
}) {
  return (
    <Notice
      tone="error"
      className={className}
      action={
        <Button size="sm" variant="secondary" disabled={busy} onClick={onRetry}>
          Try again
        </Button>
      }
    >
      {sayError(error, fallback)}
    </Notice>
  );
}
