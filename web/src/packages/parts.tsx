/**
 * The small pieces the package sheet's views share: the way back from a package, a fact with its name above it, a
 * list of lines in the code face, and the Remove button that asks once before it removes something that takes effect
 * at once.
 */
import { ArrowLeftIcon } from "@phosphor-icons/react";
import { type ReactNode, useState } from "react";
import { Button } from "../ui/components/Button";

export function Back({ label, onClick }: { label: string; onClick: () => void }) {
  return (
    <div>
      <Button variant="ghost" size="sm" onClick={onClick}>
        <ArrowLeftIcon aria-hidden="true" className="size-4" weight="bold" />
        {label}
      </Button>
    </div>
  );
}

/** A list of rows, which a person hearing the page learns the count of. */
export function PlainList({
  label,
  className,
  children,
}: {
  label: string;
  className?: string;
  children: ReactNode;
}) {
  return (
    <ul
      // biome-ignore lint/a11y/noRedundantRoles: preflight sets list-style: none, and WebKit then drops a list with no marker from the accessibility tree entirely
      role="list"
      aria-label={label}
      className={`m-0 flex list-none flex-col p-0 ${className ?? ""}`}
    >
      {children}
    </ul>
  );
}

/** One fact of a package: its name above, its value under. */
export function Fact({ label, children }: { label: string; children: ReactNode }) {
  return (
    <div className="flex min-w-0 flex-col gap-1">
      <span className="text-sm font-semibold text-secondary">{label}</span>
      <span className="min-w-0 break-words text-base text-text">{children}</span>
    </div>
  );
}

/** Paths and names, one to a line, in the code face. */
export function Lines({ label, lines }: { label: string; lines: string[] }) {
  if (lines.length === 0) return null;
  return (
    <div className="flex min-w-0 flex-col gap-1">
      <span className="text-sm font-semibold text-secondary">{label}</span>
      <PlainList label={label} className="gap-1">
        {lines.map((line) => (
          <li key={line} className="min-w-0 break-all font-mono text-sm text-text">
            {line}
          </li>
        ))}
      </PlainList>
    </div>
  );
}

/** A link to a page outside the app, which opens in a tab of its own. */
export function Outside({ href, children }: { href: string; children: ReactNode }) {
  return (
    <a
      href={href}
      target="_blank"
      rel="noreferrer"
      className="break-all text-text underline underline-offset-2 hover:text-secondary"
    >
      {children}
    </a>
  );
}

/**
 * Removes a package. With `confirm` it asks once first, because the package goes at once (a plugin); a removal that is
 * only proposed (a Pi package, or a workflow, rules or skills package) is a change a person merges, which is the
 * question already.
 */
export function RemoveButton({
  name,
  busy,
  confirm,
  onRemove,
}: {
  name: string;
  busy: boolean;
  confirm: boolean;
  onRemove: () => void;
}) {
  const [asking, setAsking] = useState(false);
  if (!asking) {
    return (
      <Button
        variant="secondary"
        size="sm"
        disabled={busy}
        aria-label={`Remove ${name}`}
        onClick={() => (confirm ? setAsking(true) : onRemove())}
      >
        Remove
      </Button>
    );
  }
  return (
    <span className="flex flex-wrap items-center gap-2">
      <span className="text-sm text-secondary">{`Remove ${name} now?`}</span>
      <Button
        variant="danger"
        size="sm"
        disabled={busy}
        aria-label={`Confirm removing ${name}`}
        onClick={() => {
          setAsking(false);
          onRemove();
        }}
      >
        Remove it
      </Button>
      <Button variant="secondary" size="sm" onClick={() => setAsking(false)}>
        Keep it
      </Button>
    </span>
  );
}
