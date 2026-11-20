/**
 * The pieces the Settings areas share: the title of the area, a section of rows, a value row, and a copy button tall enough to press. The shared `CopyButton` of `src/ui/chat` is a 28px line of code font, which
 * is too short to be a control and sets a value in the font of a label, so a secret copies itself with this one.
 */
import { CopyIcon } from "@phosphor-icons/react";
import { type ReactNode, useEffect, useRef, useState } from "react";
import { Button } from "../../ui/components/Button";
import { FieldRow } from "../../ui/page/Field";

/** The title of the chosen area at the top of the content, with the area's own actions at its right. */
export function AreaTitle({ title, actions }: { title: string; actions?: ReactNode }) {
  return (
    <div className="mb-4 flex min-h-10 flex-wrap items-center justify-between gap-3">
      <h2 className="m-0 text-xl font-semibold tracking-tight text-text">{title}</h2>
      {actions && <div className="flex flex-wrap items-center gap-2">{actions}</div>}
    </div>
  );
}

/** A group of rows under a quiet heading; the rows' hairlines divide it from what is above. */
export function Section({
  title,
  aside,
  children,
}: {
  title: string;
  aside?: ReactNode;
  children: ReactNode;
}) {
  return (
    <section className="mt-6 min-w-0 first:mt-0">
      <div className="mb-2 flex flex-wrap items-baseline justify-between gap-3">
        <h3 className="m-0 text-base font-semibold text-text">{title}</h3>
        {aside && <span className="text-sm text-muted">{aside}</span>}
      </div>
      {children}
    </section>
  );
}

/** A fact the page only shows: the label at the left, the value at the right in the font it needs. */
export function ValueRow({ label, value }: { label: string; value: ReactNode }) {
  return (
    <FieldRow label={label}>
      <span className="min-w-0 max-w-full break-all text-right text-sm text-text tabular-nums">
        {value}
      </span>
    </FieldRow>
  );
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
