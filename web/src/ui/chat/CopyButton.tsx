/**
 * The button that puts `text` on the clipboard and says "Copied" for a moment. The same button comes with every
 * message, sits in the head of every code block and beside a diff. It waits for the block it belongs to to be
 * hovered or focused (`copy-quiet` and `reveal-copy` in `styles/tailwind.css`), because it is a way of taking the
 * words away rather than part of them. Under a message it floats over the corner of it rather than sitting on a row
 * of its own (`copy-float`), so the message is as tall as its words. It is the shared `Button`, so it is the same
 * height as every other control; its accessible name is what the button is for ("Copy message"), which a screen
 * reader reads instead of the word that changes. It uses the async Clipboard API, which a page served over HTTPS or
 * from localhost may use; where the browser refuses, it says "Copy failed" instead of nothing.
 */
import { CheckIcon, CopyIcon } from "@phosphor-icons/react";
import { useEffect, useRef, useState } from "react";
import { Button } from "../components/Button";
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
    <Button
      type="button"
      variant="ghost"
      size="sm"
      onClick={() => void copy()}
      aria-label={label}
      className={cn("copy-quiet", className)}
    >
      {state === "copied" ? (
        <CheckIcon aria-hidden="true" className="size-4" />
      ) : (
        <CopyIcon aria-hidden="true" className="size-4" />
      )}
      <span aria-live="polite">
        {state === "copied" ? "Copied" : state === "failed" ? "Copy failed" : "Copy"}
      </span>
    </Button>
  );
}
