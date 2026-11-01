/**
 * The small pieces the pages of Rules, Automations, Retro, Experiments and Decision model share: the page header with
 * its actions, a box with a heading, a labelled field, a notice, a chip that switches, a plain select, and the list
 * with the detail beside it. They are written for Moebius out of the components of `ui/`; the iced app's
 * `widgets::page` and `widgets::form` are what they follow.
 */
import { type ReactNode, useId } from "react";
import { Tag, type TagTone } from "../../ui/chat/Tag";
import { cn } from "../../ui/lib/cn";

/** The title of a page with the sentence under it and the page's actions on the right. */
export function PageHeader({
  breadcrumb,
  title,
  context,
  actions,
}: {
  breadcrumb?: string;
  title: string;
  context?: ReactNode;
  actions?: ReactNode;
}) {
  return (
    <header className="mb-double flex flex-wrap items-start justify-between gap-base">
      <div className="min-w-0">
        {breadcrumb && <div className="font-mono text-[12px] text-low">{breadcrumb}</div>}
        <h1 className="text-[19px] font-bold text-high">{title}</h1>
        {context && <p className="mt-half text-sm text-low">{context}</p>}
      </div>
      {actions && <div className="flex items-center gap-base">{actions}</div>}
    </header>
  );
}

/** A box with a heading, an aside on the heading's right and its content. */
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
    <section className={cn("rounded-xl border bg-panel p-double", className)} aria-label={title}>
      {(title || aside) && (
        <div className="mb-base flex flex-wrap items-center justify-between gap-base">
          {title && <h2 className="text-[15px] font-bold text-high">{title}</h2>}
          {aside && <div className="flex items-center gap-base text-xs text-low">{aside}</div>}
        </div>
      )}
      <div className="flex flex-col gap-base">{children}</div>
    </section>
  );
}

/** A sentence of a box in the plain colour. */
export function Body({ children }: { children: ReactNode }) {
  return <p className="text-sm text-normal">{children}</p>;
}

/** A field with its label above it; `children` receives the id the label points at. */
export function Field({
  label,
  children,
  className,
}: {
  label: string;
  children: (id: string) => ReactNode;
  className?: string;
}) {
  const id = useId();
  return (
    <div className={cn("flex min-w-0 flex-1 flex-col gap-half", className)}>
      <label htmlFor={id} className="text-xs font-semibold text-low">
        {label}
      </label>
      {children(id)}
    </div>
  );
}

const CONTROL =
  "rounded-md border border-line-strong bg-recessed px-3 py-2 text-sm text-high placeholder:text-low focus-visible:border-accent focus-visible:outline-none disabled:opacity-50";

/** A multi-line field in the code font or the plain one. */
export function TextArea({
  mono,
  className,
  ...props
}: React.ComponentProps<"textarea"> & { mono?: boolean }) {
  return (
    <textarea
      className={cn(CONTROL, "w-full resize-y", mono && "font-mono text-xs", className)}
      {...props}
    />
  );
}

/** A native select, which keeps the keyboard and screen reader behaviour for free. */
export function Select({ className, ...props }: React.ComponentProps<"select">) {
  return <select className={cn(CONTROL, "h-9 w-full py-0", className)} {...props} />;
}

/** A notice under a form: what came of the last request. A refusal is an alert in red. */
export function Notice({ tone = "ok", children }: { tone?: "ok" | "error"; children: ReactNode }) {
  return (
    <p
      role={tone === "error" ? "alert" : "status"}
      className={cn("text-sm", tone === "error" ? "text-error" : "text-success")}
    >
      {children}
    </p>
  );
}

/** One of a few choices that is on or off, such as "Lower is better". */
export function Chip({
  active,
  onClick,
  children,
}: {
  active: boolean;
  onClick: () => void;
  children: ReactNode;
}) {
  return (
    <button
      type="button"
      aria-pressed={active}
      onClick={onClick}
      className={cn(
        "rounded-full border px-plusfifty py-half text-xs transition-colors",
        active
          ? "border-accent-border bg-accent-fill text-high"
          : "border-line-strong bg-card text-normal hover:bg-card-hover",
      )}
    >
      {children}
    </button>
  );
}

/** A card of a list on the left of a page: a title, tags, a line, and whether it is the chosen one. */
export function ListCard({
  title,
  tags,
  line,
  alert,
  chosen,
  onClick,
}: {
  title: string;
  tags: Array<{ label: string; tone: TagTone }>;
  line: string;
  /** The line is a failure. */
  alert?: boolean;
  chosen: boolean;
  onClick: () => void;
}) {
  return (
    <button
      type="button"
      aria-current={chosen ? "true" : undefined}
      onClick={onClick}
      className={cn(
        "flex w-full flex-col gap-half rounded-lg border p-plusfifty text-left transition-colors",
        chosen ? "border-accent-border bg-accent-fill" : "bg-surface hover:bg-card",
      )}
    >
      <span className="flex items-start justify-between gap-base">
        <span className="min-w-0 break-words text-sm font-semibold text-high">{title}</span>
        <span className="flex shrink-0 gap-half">
          {tags.map((tag) => (
            <Tag key={tag.label} tone={tag.tone}>
              {tag.label}
            </Tag>
          ))}
        </span>
      </span>
      <span className={cn("break-words text-xs", alert ? "text-error" : "text-low")}>{line}</span>
    </button>
  );
}

/** A list with the detail of the chosen item beside it; on a narrow screen they stack. */
export function ListDetail({ list, detail }: { list: ReactNode; detail: ReactNode }) {
  return (
    <div className="flex flex-col gap-double lg:flex-row lg:items-start">
      <div className="flex w-full shrink-0 flex-col gap-base lg:w-[300px]">{list}</div>
      <div className="flex min-w-0 flex-1 flex-col gap-double">{detail}</div>
    </div>
  );
}

/** A table with a head row; the cells are the caller's. */
export function Table({ head, children }: { head: string[]; children: ReactNode }) {
  return (
    <div className="overflow-x-auto">
      <table className="w-full border-collapse text-left text-sm">
        <thead>
          <tr className="border-b text-xs text-low">
            {head.map((name) => (
              <th key={name} scope="col" className="px-base py-half font-semibold">
                {name}
              </th>
            ))}
          </tr>
        </thead>
        <tbody>{children}</tbody>
      </table>
    </div>
  );
}

export const CELL = "border-b px-base py-base align-top text-normal";

/** The sentence a failed request shows: the server's own. */
export function sayError(error: unknown, fallback: string): string {
  return error instanceof Error && error.message !== "" ? error.message : fallback;
}
