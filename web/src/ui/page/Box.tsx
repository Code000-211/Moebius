/** A box of a page: a heading, a line that sums it up at its right, and the content. The Usage, Settings and Projects pages are made of these. */
import type { ReactNode } from "react";
import { cn } from "../lib/cn";

export function Box({
  title,
  aside,
  children,
  className,
}: {
  title?: string;
  aside?: ReactNode;
  children: ReactNode;
  className?: string;
}) {
  return (
    <section className={cn("rounded-xl border bg-panel p-double", className)}>
      {(title || aside) && (
        <header className="mb-[12px] flex flex-wrap items-baseline justify-between gap-base">
          {title && <h2 className="m-0 text-[14px] font-bold text-high">{title}</h2>}
          {aside && <span className="font-mono text-[12px] text-low">{aside}</span>}
        </header>
      )}
      {children}
    </section>
  );
}
