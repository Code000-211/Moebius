/**
 * The pieces the Settings page and its webhooks box share: a value that sits beside its label in a three-column grid,
 * and a copy button tall enough to press. The shared `CopyButton` of `src/ui/chat` is a 28px line of code font, which
 * is too short to be a control and sets a value in the font of a label, so a secret copies itself with this one.
 */
import { CopyIcon } from "@phosphor-icons/react";
import { type ReactNode, useEffect, useRef, useState } from "react";
import { Button } from "../../ui/components/Button";

/** A row of a `KeyValueGrid`: the label at the left, the value in an inset box beside it. */
export function KeyValue({ label, value }: { label: string; value: ReactNode }) {
  return (
    <div className="contents">
      <dt className="text-sm font-semibold text-secondary sm:pt-2">{label}</dt>
      <dd className="m-0 min-w-0 break-words rounded-control bg-sunken px-3 py-2 text-sm text-text tabular-nums sm:col-span-2">
        {value}
      </dd>
    </div>
  );
}

/** Values laid out two to a card: the label over the value while the page is narrow, beside it once it is wide. */
export function KeyValueGrid({ children }: { children: ReactNode }) {
  return <dl className="m-0 grid gap-x-4 gap-y-4 sm:grid-cols-3">{children}</dl>;
}

/** A value that is character-precise — a URL, a secret, a path — set in the code font. */
export function Code({ children }: { children: ReactNode }) {
  return <span className="font-mono">{children}</span>;
}

/** The button that puts a value on the clipboard and says what it did, at the height of every other control. */
export function CopyValue({ text, label }: { text: string; label: string }) {
  const [state, setState] = useState<"idle" | "copied" | "failed">("idle");
  const timer = useRef<ReturnType<typeof setTimeout> | undefined>(undefined);
  useEffect(() => () => clearTimeout(timer.current), []);

  async function copy() {
    let next: "idle" | "copied" | "failed" = "copied";
    try {
      await navigator.clipboard.writeText(text);
    } catch {
      next = "failed";
    }
    setState(next);
    clearTimeout(timer.current);
    timer.current = setTimeout(() => setState("idle"), 1500);
  }

  return (
    <Button
      size="sm"
      variant="secondary"
      aria-label={label}
      disabled={state !== "idle"}
      onClick={() => void copy()}
    >
      <CopyIcon className="size-4" aria-hidden="true" />
      <span aria-live="polite">
        {state === "copied" ? "Copied" : state === "failed" ? "Copy failed" : label}
      </span>
    </Button>
  );
}
