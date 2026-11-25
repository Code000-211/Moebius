/**
 * The notice of a frozen repository: nothing is reviewed, merged or started in it until a person unfreezes it. The card
 * window and the repository's "Working with others" box say it in the same words, with the reason the server gave.
 */
import type { Frozen } from "../../api/types/board";
import { Notice } from "../../ui/page/Notice";

/** Where the freeze came from, in words. */
const SOURCE: Record<Frozen["source"], string> = {
  manual: "A person froze it.",
  label: "The label factory:freeze is on an issue on the forge.",
  automatic: "A breaker froze it.",
};

export function FrozenNotice({
  frozen,
  className,
}: {
  frozen: Frozen | null | undefined;
  className?: string;
}) {
  if (!frozen) return null;
  return (
    <Notice tone="error" className={className}>
      <span data-testid="frozen-notice">
        <span className="font-semibold">{`Frozen: ${frozen.reason}`}</span>
        {` ${SOURCE[frozen.source] ?? ""} Nothing is reviewed, merged or started until a person unfreezes it.`}
      </span>
    </Notice>
  );
}
