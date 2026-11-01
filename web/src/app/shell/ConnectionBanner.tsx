/**
 * The strip over the page while the server cannot be reached: the event stream is down or the session cannot be
 * read. What was shown stays; the app tries again by itself and the button does not wait for the next try.
 */
import { WarningIcon } from "@phosphor-icons/react";
import { Button } from "../../ui/components/Button";

export function ConnectionBanner({ onRetry }: { onRetry: () => void }) {
  return (
    <div
      role="alert"
      className="flex min-h-12 items-center gap-3 bg-attention-fill px-6 py-3 text-base text-attention-text"
    >
      <WarningIcon className="size-5 shrink-0" weight="bold" aria-hidden="true" />
      <span className="flex-1">Cannot reach the server. It is trying again.</span>
      <Button variant="secondary" size="sm" onClick={onRetry}>
        Retry now
      </Button>
    </div>
  );
}
