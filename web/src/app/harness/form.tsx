/**
 * The pieces the pages of the harness configuration and of the setup (Agents, Skills, MCP servers, Models, Plugins)
 * share: a box with a heading, a labelled field, native selects and text areas in the Deep Velvet look, a notice line, a
 * chip that toggles, and the layout of a list beside its editor. They are small and plain; the board and the chat have
 * their own components in `src/ui`.
 */
import { type ComponentProps, type ReactNode, useId, useState } from "react";
import { Button } from "../../ui/components/Button";
import { Input } from "../../ui/components/Input";
import { cn } from "../../ui/lib/cn";

/** A white box of a page: a heading with a muted word on its right, and its content. */
export function Box({
  title,
  aside,
  actions,
  children,
  className,
}: {
  title?: string;
  aside?: ReactNode;
  actions?: ReactNode;
  children: ReactNode;
  className?: string;
}) {
  return (
    <section className={cn("rounded-xl border border-line bg-surface p-double", className)}>
      {(title || aside || actions) && (
        <div className="mb-plusfifty flex items-baseline justify-between gap-base">
          <h2 className="m-0 text-[14px] font-semibold text-high">{title}</h2>
          {aside && <span className="font-mono text-[11.5px] text-low">{aside}</span>}
          {actions}
        </div>
      )}
      <div className="flex flex-col gap-plusfifty">{children}</div>
    </section>
  );
}

/** A control with its label above it and, when given, a muted word on the right of the label. */
export function Field({
  label,
  hint,
  children,
  className,
}: {
  label: string;
  hint?: ReactNode;
  children: (id: string) => ReactNode;
  className?: string;
}) {
  const id = useId();
  return (
    <div className={cn("flex min-w-0 flex-col gap-half", className)}>
      <div className="flex items-baseline justify-between gap-base">
        <label htmlFor={id} className="text-xs font-semibold text-normal">
          {label}
        </label>
        {hint && <span className="font-mono text-[11px] text-low">{hint}</span>}
      </div>
      {children(id)}
    </div>
  );
}

export function TextInput({
  mono,
  className,
  ...props
}: ComponentProps<typeof Input> & { mono?: boolean }) {
  return <Input className={cn(mono && "font-mono text-[12.5px]", className)} {...props} />;
}

/** A native select; the options are `[value, label]` pairs or plain strings. */
export function Select({
  options,
  value,
  onChange,
  className,
  ...props
}: {
  options: Array<string | [string, string]>;
  value: string;
  onChange: (value: string) => void;
  className?: string;
} & Omit<ComponentProps<"select">, "onChange" | "value">) {
  return (
    <select
      value={value}
      onChange={(event) => onChange(event.target.value)}
      className={cn(
        "h-9 w-full rounded-md border border-line-strong bg-recessed px-2 text-sm text-high focus-visible:border-accent focus-visible:outline-none disabled:opacity-50",
        className,
      )}
      {...props}
    >
      {options.map((option) => {
        const [optionValue, label] = typeof option === "string" ? [option, option] : option;
        return (
          <option key={optionValue} value={optionValue}>
            {label}
          </option>
        );
      })}
    </select>
  );
}

/** A text area; `mono` is for code and Markdown. */
export function TextArea({
  mono = true,
  rows = 8,
  className,
  ...props
}: ComponentProps<"textarea"> & { mono?: boolean }) {
  return (
    <textarea
      rows={rows}
      spellCheck={false}
      className={cn(
        "w-full resize-y rounded-md border border-line-strong bg-recessed p-2 text-high placeholder:text-low focus-visible:border-accent focus-visible:outline-none disabled:opacity-60",
        mono ? "font-mono text-[12.5px] leading-[1.5]" : "text-sm",
        className,
      )}
      {...props}
    />
  );
}

/** A line under a form: what went wrong in red, what was done in green. */
export function Notice({ tone, children }: { tone: "error" | "ok"; children: ReactNode }) {
  return (
    <p
      role={tone === "error" ? "alert" : "status"}
      className={cn(
        "m-0 text-xs [overflow-wrap:anywhere]",
        tone === "error" ? "text-error" : "text-success",
      )}
    >
      {children}
    </p>
  );
}

/** The line that says an edit was committed, with the short id; or that it changed nothing. */
export function Committed({ commit }: { commit: string | null | undefined }) {
  return (
    <Notice tone="ok">
      {commit ? `Saved as commit ${commit}.` : "Saved. It changed nothing, so there is no commit."}
    </Notice>
  );
}

/** A choice that is on or off; pressed means chosen. */
export function Chip({
  chosen,
  broken,
  onClick,
  children,
}: {
  chosen: boolean;
  broken?: boolean;
  onClick: () => void;
  children: ReactNode;
}) {
  return (
    <button
      type="button"
      aria-pressed={chosen}
      onClick={onClick}
      className={cn(
        "inline-flex items-center rounded-full border px-plusfifty py-half font-mono text-xs transition-colors",
        chosen
          ? "border-accent-border bg-accent-fill text-high"
          : "border-line-strong bg-card text-normal hover:bg-card-hover",
        broken && "border-red-border text-error",
      )}
    >
      {children}
    </button>
  );
}

/** One card of a list: a name, tags, and a line under it that is red when something is wrong. */
export function ListCard({
  name,
  tags,
  line,
  red,
  chosen,
  onClick,
}: {
  name: string;
  tags?: ReactNode;
  line: string;
  red?: boolean;
  chosen: boolean;
  onClick?: () => void;
}) {
  return (
    <button
      type="button"
      data-testid="list-card"
      aria-current={chosen ? "true" : undefined}
      disabled={!onClick}
      onClick={onClick}
      className={cn(
        "flex w-full min-w-0 flex-col gap-half rounded-lg border p-plusfifty text-left transition-colors",
        chosen
          ? "border-accent-border bg-accent-fill"
          : "border-line bg-card hover:bg-card-hover disabled:hover:bg-card",
      )}
    >
      <span className="flex min-w-0 items-center gap-base">
        <span className="truncate text-sm font-semibold text-high">{name}</span>
        {tags}
      </span>
      {line && (
        <span
          className={cn("truncate font-mono text-[11.5px]", red ? "text-error" : "text-low")}
          title={line}
        >
          {line}
        </span>
      )}
    </button>
  );
}

/** A list of cards on the left and the editor of the one chosen on the right; one column on a narrow screen. */
export function ListEditor({ list, children }: { list: ReactNode; children: ReactNode }) {
  return (
    <div className="grid items-start gap-double lg:grid-cols-[18rem_minmax(0,1fr)]">
      <div className="flex flex-col gap-base">{list}</div>
      <div className="flex min-w-0 flex-col gap-double">{children}</div>
    </div>
  );
}

/**
 * The head of an editor: the name (an input while a new item is named), and Delete (which asks first) and Save. Save is
 * left out of `actions` for a read-only item.
 */
export function EditorHead({
  name,
  naming,
  exists,
  deleteLabel,
  namePlaceholder,
  onName,
  saving,
  canSave,
  onSave,
  onDelete,
  deleting,
  readOnly,
}: {
  name: string;
  naming: boolean;
  exists: boolean;
  deleteLabel: string;
  namePlaceholder: string;
  onName: (name: string) => void;
  saving: boolean;
  canSave: boolean;
  onSave: () => void;
  onDelete: () => void;
  deleting: boolean;
  readOnly?: ReactNode;
}) {
  const nameId = useId();
  return (
    <div className="flex flex-wrap items-center justify-between gap-base">
      {naming ? (
        <>
          <label htmlFor={nameId} className="sr-only">
            {namePlaceholder}
          </label>
          <TextInput
            id={nameId}
            mono
            autoFocus
            value={name}
            placeholder={namePlaceholder}
            onChange={(event) => onName(event.target.value)}
            className="max-w-xs"
          />
        </>
      ) : (
        <h2 className="m-0 truncate font-mono text-[15px] font-semibold text-high">{name}</h2>
      )}
      {readOnly ?? (
        <ActionRow
          exists={exists}
          deleteLabel={deleteLabel}
          name={name}
          saving={saving}
          canSave={canSave}
          onSave={onSave}
          onDelete={onDelete}
          deleting={deleting}
        />
      )}
    </div>
  );
}

/** Delete (asking first) and Save. */
export function ActionRow({
  exists,
  deleteLabel,
  name,
  saving,
  canSave,
  onSave,
  onDelete,
  deleting,
}: {
  exists: boolean;
  deleteLabel: string;
  name: string;
  saving: boolean;
  canSave: boolean;
  onSave: () => void;
  onDelete: () => void;
  deleting: boolean;
}) {
  const [asking, setAsking] = useState(false);
  if (asking) {
    return (
      <div className="flex flex-wrap items-center gap-base">
        <span className="text-xs text-normal">Delete {name}?</span>
        <Button variant="outline" size="sm" onClick={() => setAsking(false)}>
          Keep
        </Button>
        <Button
          variant="destructive"
          size="sm"
          disabled={deleting}
          onClick={() => {
            setAsking(false);
            onDelete();
          }}
        >
          {deleteLabel}
        </Button>
      </div>
    );
  }
  return (
    <div className="flex items-center gap-base">
      {exists && (
        <Button variant="outline" size="sm" onClick={() => setAsking(true)}>
          {deleteLabel}
        </Button>
      )}
      <Button size="sm" disabled={!canSave || saving} onClick={onSave}>
        {saving ? "Saving…" : "Save"}
      </Button>
    </div>
  );
}
