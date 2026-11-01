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
      className="flex items-center gap-base border-b border-amber/40 bg-amber-fill px-double py-base text-sm text-amber"
    >
      <WarningIcon className="size-icon-sm shrink-0" weight="bold" />
      <span className="flex-1">Cannot reach the server. Trying again…</span>
      <Button variant="outline" size="xs" onClick={onRetry}>
        Retry now
      </Button>
    </div>
  );
}
