/**
 * The controls of a form, in one place: a labelled field, and the text, multi-line and choice controls that go in it.
 * A field is borderless on the sunken colour and at least 40px tall; its label sits above it and its hint or its error
 * under it, so the explanation of a constraint is where the constraint is.
 *
 * `Field` wraps its control in a `<label>`, so a screen reader and a test find the control by its label. A caller that
 * needs the id for itself takes `children` as a function instead and gets the id. The hint and the error sit outside
 * the label and are named by `aria-describedby` instead, so they are still read out without becoming part of the
 * control's name.
 *
 * A choice of one opens this app's own menu (`ui/components/DropdownMenu.tsx`) rather than the list the operating
 * system draws, so that one screen can hold a field, a toolbar picker and a composer pill that all look and answer the
 * same. The standard used to allow the native `<select>` for a short list; the two lists beside each other did not
 * read as one app.
 */
import { CaretDownIcon } from "@phosphor-icons/react";
import { type ComponentProps, type JSX, type ReactNode, useId } from "react";
import {
  DropdownMenu,
  DropdownMenuContent,
  DropdownMenuRadioGroup,
  DropdownMenuRadioItem,
  DropdownMenuTrigger,
} from "../components/DropdownMenu";
import { cn } from "../lib/cn";
import { CONTROL, FieldIds, useFieldIds } from "../lib/field";

/** The classes every control shares live in `ui/lib/field.ts`; they are re-exported here for the pages that already
 * take them from this module. */
export { CONTROL } from "../lib/field";

export function Field({
  label,
  hint,
  error,
  children,
  className,
}: {
  label: string;
  hint?: ReactNode;
  /** What went wrong with this field, beside it, with what to do next. */
  error?: string;
  children: ReactNode | ((id: string) => ReactNode);
  className?: string;
}) {
  const id = useId();
  const control = typeof children === "function" ? children(id) : children;
  const labelId = `${id}-label`;
  const noteId = `${id}-note`;
  const note = error ?? hint;
  return (
    <div className={cn("flex min-w-0 flex-col gap-2", className)}>
      <FieldIds value={{ label: labelId, note: note ? noteId : undefined }}>
        <label
          htmlFor={typeof children === "function" ? id : undefined}
          className="flex min-w-0 flex-col gap-2"
        >
          <span id={labelId} className="text-sm font-semibold text-secondary">
            {label}
          </span>
          {control}
        </label>
        {note && (
          <span id={noteId} className={cn("text-sm", error ? "text-error-text" : "text-secondary")}>
            {note}
          </span>
        )}
      </FieldIds>
    </div>
  );
}

/** A multi-line field, which grows when the content needs it. `mono` is for a script, a path or a diff. */
export function TextArea({
  mono,
  className,
  ...props
}: React.ComponentProps<"textarea"> & { mono?: boolean }) {
  const named = useFieldIds({
    label: props["aria-label"],
    labelledBy: props["aria-labelledby"],
    describedBy: props["aria-describedby"],
  });
  return (
    <textarea
      spellCheck={mono ? false : undefined}
      className={cn(
        CONTROL,
        "h-auto min-h-20 resize-y leading-relaxed",
        mono && "font-mono text-sm",
        className,
      )}
      aria-labelledby={named.labelledBy}
      aria-describedby={named.describedBy}
      {...props}
    />
  );
}

/** What a `Select` is given: its choices, the one that stands, and what to do when another is picked. */
type SelectProps = Omit<ComponentProps<"button">, "onChange" | "value" | "children" | "type"> & {
  /** Each choice is a value on its own, or a value with the words that show it. */
  options: Array<string | [string, string]>;
  value: string;
  onChange: (value: string) => void;
};

/**
 * A choice of one from a list, drawn like every other field and opened like every other list of choices: the app's
 * menu, so the rows are the same 40px rows with the chosen one semibold on the selected fill, in the colours of the
 * theme rather than of the operating system. The trigger shows what is chosen now.
 *
 * A long list that needs searching is a different thing — the composer's `PillSelect` carries a field for it — and two
 * or three choices that are always worth seeing are buttons in a row, not a field.
 */
export function Select({ options, value, onChange, className, ...rest }: SelectProps): JSX.Element {
  const {
    "aria-label": label,
    "aria-labelledby": labelledBy,
    "aria-describedby": describedBy,
    ...own
  } = rest;
  const named = useFieldIds({ label, labelledBy, describedBy });
  const choices = options.map((option): [string, string] =>
    typeof option === "string" ? [option, option] : option,
  );
  const shown = choices.find(([each]) => each === value)?.[1] ?? value;
  return (
    <DropdownMenu>
      <DropdownMenuTrigger
        {...own}
        aria-label={label}
        aria-labelledby={named.labelledBy}
        aria-describedby={named.describedBy}
        className={cn(
          CONTROL,
          "flex cursor-pointer items-center justify-between gap-3 py-2 pr-3 text-left",
          className,
        )}
      >
        <span className="min-w-0 truncate" title={shown}>
          {shown}
        </span>
        <CaretDownIcon aria-hidden="true" className="size-4 shrink-0 text-secondary" />
      </DropdownMenuTrigger>
      <DropdownMenuContent
        align="start"
        className="min-w-(--radix-dropdown-menu-trigger-width) max-w-(--radix-dropdown-menu-content-available-width)"
      >
        <DropdownMenuRadioGroup value={value} onValueChange={onChange}>
          {choices.map(([each, words]) => (
            <DropdownMenuRadioItem key={each} value={each} className="text-sm">
              {words}
            </DropdownMenuRadioItem>
          ))}
        </DropdownMenuRadioGroup>
      </DropdownMenuContent>
    </DropdownMenu>
  );
}
