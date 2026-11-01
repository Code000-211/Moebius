/**
 * A small button that puts `text` on the clipboard and says "Copied" for a moment. The same button is under every
 * message and in the head of every code block. It uses the async Clipboard API, which a page served over HTTPS or from
 * localhost may use; where the browser refuses, the button says "Copy failed" instead of nothing.
 */
import { CheckIcon, CopyIcon } from "@phosphor-icons/react";
import { useEffect, useRef, useState } from "react";
import { cn } from "../lib/cn";

type State = "idle" | "copied" | "failed";

export function CopyButton({
  text,
  label = "Copy",
  className,
}: {
  text: string;
  /** What the button is for, such as "Copy message"; also its accessible name. */
  label?: string;
  className?: string;
}) {
  const [state, setState] = useState<State>("idle");
  const timer = useRef<ReturnType<typeof setTimeout> | undefined>(undefined);
  useEffect(() => () => clearTimeout(timer.current), []);

  async function copy() {
    let next: State = "copied";
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
    <button
      type="button"
      onClick={() => void copy()}
      aria-label={label}
      title={label}
      className={cn(
        "inline-flex items-center gap-half rounded-sm px-half py-half font-mono text-xs text-low transition-colors hover:bg-card hover:text-high",
        className,
      )}
    >
      {state === "copied" ? (
        <CheckIcon className="size-icon-xs" />
      ) : (
        <CopyIcon className="size-icon-xs" />
      )}
      <span aria-live="polite">
        {state === "copied" ? "Copied" : state === "failed" ? "Copy failed" : "Copy"}
      </span>
    </button>
  );
}
