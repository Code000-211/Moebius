/** A labelled field of a form: the label wraps the control, so a screen reader and a test find the control by it. */
import type { ReactNode } from "react";
import { cn } from "../lib/cn";

export function Field({
  label,
  hint,
  children,
  className,
}: {
  label: string;
  hint?: ReactNode;
  children: ReactNode;
  className?: string;
}) {
  return (
    // biome-ignore lint/a11y/noLabelWithoutControl: the control is one of the children
    <label className={cn("flex flex-col gap-half text-xs font-semibold text-normal", className)}>
      {label}
      {children}
      {hint && <span className="text-[12px] font-normal text-low">{hint}</span>}
    </label>
  );
}

/** The classes of a plain `<textarea>` or `<select>` next to the `Input`. */
export const CONTROL =
  "rounded-md border border-line-strong bg-recessed px-3 py-2 text-sm font-normal text-high placeholder:text-low focus-visible:border-accent focus-visible:outline-none disabled:cursor-not-allowed disabled:opacity-50";
