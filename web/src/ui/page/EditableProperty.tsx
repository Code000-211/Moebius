/**
 * A property a person edits where it stands, in the right column (pattern 2 of `docs/design-drafts/README.md`): its
 * value is a button that opens the value's own editor — a choice or a person from the app's menu, a date, a number or a
 * line of text in a field put in the button's place — and saves that one value alone: on a choice, on Enter, or when
 * the field is left. Escape leaves the field without saving. What the server refused is said under the property and is
 * the button's description, so a person hearing the page is told it where they are; the value shown stays the one the
 * server has, since a page reads it again after a save. `ListProperty` is a property of several values, such as
 * labels: each is removed by its own button and one is added through a field.
 *
 * Saving is the caller's: `onSave` sends the request and settles when the server answered, and an error it throws is
 * the refusal (the API's `ApiError` carries the server's sentence as its message).
 */
import { CaretDownIcon, PlusIcon, XIcon } from "@phosphor-icons/react";
import { type ReactNode, useId, useRef, useState } from "react";
import {
  DropdownMenu,
  DropdownMenuCheckboxItem,
  DropdownMenuContent,
  DropdownMenuRadioGroup,
  DropdownMenuRadioItem,
  DropdownMenuTrigger,
} from "../components/DropdownMenu";
import { cn } from "../lib/cn";
import { CONTROL } from "../lib/field";
import { Notice } from "./Notice";

/** What a value opens, with the value it stands at and how it is saved. */
export type PropertyEditor =
  | {
      kind: "choice";
      /** The choices, each a value and the words that show it. */
      options: Array<[string, string]>;
      value: string;
      onSave: (value: string) => Promise<unknown>;
    }
  | {
      kind: "person";
      /** The people who can be chosen, each a value and the name that shows it. */
      people: Array<[string, string]>;
      value: string | null;
      /** The words of the row that chooses nobody, such as "No one". */
      none: string;
      onSave: (value: string | null) => Promise<unknown>;
    }
  | {
      kind: "people";
      /** The people who can be chosen, each a handle and the name that shows it. */
      people: Array<[string, string]>;
      /** The handles chosen now. */
      value: string[];
      /** Saves the handles chosen after a person was added or taken off. */
      onSave: (value: string[]) => Promise<unknown>;
    }
  | {
      kind: "choices";
      /** What can be chosen besides the fixed values, each a value and the words that show it. */
      options: Array<[string, string]>;
      /** The values chosen now, not counting the fixed ones. */
      value: string[];
      /** Values that are always among the chosen: shown first, checked and not to be taken off. */
      fixed: string[];
      /** Saves the values chosen after one was added or taken off. */
      onSave: (value: string[]) => Promise<unknown>;
    }
  | {
      kind: "date";
      /** The date as `YYYY-MM-DD`, or none. */
      value: string | null;
      onSave: (value: string | null) => Promise<unknown>;
    }
  | {
      kind: "number";
      value: number | null;
      /** What to say of a number that cannot be saved, or null when it can; asked before anything is sent. */
      invalid?: (value: number) => string | null;
      onSave: (value: number | null) => Promise<unknown>;
    }
  | {
      kind: "text";
      value: string;
      onSave: (value: string) => Promise<unknown>;
    };

/** The classes of a value that opens its editor: the row's width, quiet until it is pointed at. */
const VALUE =
  "-ml-1.5 flex min-h-7 w-full min-w-0 cursor-pointer items-center gap-1.5 rounded-control bg-transparent px-1.5 text-left text-sm text-text transition-colors duration-(--motion-routine) ease-out hover:bg-hover aria-disabled:cursor-progress aria-disabled:opacity-70";

/** The classes of a small icon button beside a value. */
const SMALL =
  "inline-flex size-7 shrink-0 cursor-pointer items-center justify-center rounded-control bg-transparent text-secondary hover:bg-hover hover:text-text aria-disabled:cursor-progress aria-disabled:opacity-45";

/** What a refusal says: the server's sentence, or one for a failure that said nothing. */
function refusalOf(error: unknown): string {
  return error instanceof Error && error.message !== "" ? error.message : "Could not save this.";
}

/**
 * One save at a time, and the refusal of the last one until the next is sent. While one is sent the controls say they
 * are disabled rather than being disabled, so the focus a person had on them stays where it was.
 */
function useSave() {
  const [pending, setPending] = useState(false);
  const [refusal, setRefusal] = useState<string | null>(null);
  const run = async (save: () => Promise<unknown>) => {
    setPending(true);
    setRefusal(null);
    try {
      await save();
    } catch (error) {
      setRefusal(refusalOf(error));
    } finally {
      setPending(false);
    }
  };
  return { pending, refusal, run };
}

/** The row of a property: its name, then its value, with the refusal of its last save under both. */
function Row({
  nameId,
  name,
  refusal,
  refusalId,
  children,
}: {
  nameId: string;
  name: string;
  refusal: string | null;
  refusalId: string;
  children: ReactNode;
}) {
  return (
    <div className="flex flex-col gap-1">
      <div className="grid min-h-8 grid-cols-[6rem_minmax(0,1fr)] items-center gap-2 px-1 text-sm">
        <span id={nameId} className="text-secondary">
          {name}
        </span>
        <div className="flex min-w-0 items-center gap-1">{children}</div>
      </div>
      {refusal && (
        <div id={refusalId} className="px-1">
          <Notice tone="error" className="text-sm">
            {refusal}
          </Notice>
        </div>
      )}
    </div>
  );
}

/** A field in the value's place: Enter or leaving it saves what was typed, Escape gives the value back unchanged. */
function InlineField({
  type,
  initial,
  labelledBy,
  onCommit,
  onCancel,
}: {
  type: "text" | "number" | "date";
  initial: string;
  labelledBy: string;
  onCommit: (typed: string) => void;
  onCancel: () => void;
}) {
  const [typed, setTyped] = useState(initial);
  // Escape and Enter both end the field, and the blur that follows must not end it a second time.
  const done = useRef(false);
  const finish = (save: boolean) => {
    if (done.current) return;
    done.current = true;
    if (save && typed !== initial) onCommit(typed);
    else onCancel();
  };
  return (
    <input
      // biome-ignore lint/a11y/noAutofocus: the field takes the place of the button a person just pressed
      autoFocus
      // A number is typed into a text field, so that "#12", the way an issue is written, reaches the parse whole.
      type={type === "number" ? "text" : type}
      value={typed}
      inputMode={type === "number" ? "numeric" : undefined}
      aria-labelledby={labelledBy}
      onChange={(event) => setTyped(event.target.value)}
      onBlur={() => finish(true)}
      onKeyDown={(event) => {
        if (event.key === "Enter" && !event.nativeEvent.isComposing) {
          event.preventDefault();
          finish(true);
        } else if (event.key === "Escape") {
          // The field's own Escape: the dock or a sheet behind it is not closed with it.
          event.preventDefault();
          event.stopPropagation();
          finish(false);
        }
      }}
      className={cn(CONTROL, "-ml-1.5 min-h-7 py-0.5 text-sm")}
    />
  );
}

/**
 * Puts the focus back on a button once it is drawn again, unless the person has already moved on: the focus is then
 * on another control, and taking it would send their next key to the wrong place.
 */
function giveBackFocus(target: { current: HTMLElement | null }) {
  requestAnimationFrame(() => {
    const now = document.activeElement;
    if (now === null || now === document.body) target.current?.focus();
  });
}

/** The value as a field shows it, and what the typing means once it is saved. */
function typedOf(editor: Extract<PropertyEditor, { kind: "text" | "number" | "date" }>): string {
  if (editor.kind === "text") return editor.value;
  return editor.value === null ? "" : String(editor.value);
}

/**
 * One property whose value opens its own editor. `children` is the value as it reads (a mark and words); `after` is
 * what stands beside it, such as a link to what the value names.
 */
export function EditableProperty({
  name,
  editor,
  children,
  after,
}: {
  name: string;
  editor: PropertyEditor;
  children: ReactNode;
  after?: ReactNode;
}) {
  const id = useId();
  const nameId = `${id}-name`;
  const valueId = `${id}-value`;
  const refusalId = `${id}-refusal`;
  const { pending, refusal, run } = useSave();
  const [editing, setEditing] = useState(false);
  const [open, setOpen] = useState(false);
  const button = useRef<HTMLButtonElement>(null);
  const described = refusal ? refusalId : undefined;

  const value = (
    <span id={valueId} className="flex min-w-0 flex-1 items-center gap-1.5 break-words">
      {children}
    </span>
  );

  if (editor.kind === "choice" || editor.kind === "person") {
    const options =
      editor.kind === "choice"
        ? editor.options
        : [...editor.people, ["", editor.none] as [string, string]];
    const current = editor.value ?? "";
    const choose = (next: string) => {
      if (next === current) return;
      if (editor.kind === "choice") void run(() => editor.onSave(next));
      else void run(() => editor.onSave(next === "" ? null : next));
    };
    return (
      <Row nameId={nameId} name={name} refusal={refusal} refusalId={refusalId}>
        <DropdownMenu open={open} onOpenChange={(next) => setOpen(next && !pending)}>
          <DropdownMenuTrigger
            aria-disabled={pending || undefined}
            aria-busy={pending || undefined}
            aria-labelledby={`${nameId} ${valueId}`}
            aria-describedby={described}
            className={VALUE}
          >
            {value}
            <CaretDownIcon aria-hidden="true" className="size-3.5 shrink-0 text-secondary" />
          </DropdownMenuTrigger>
          <DropdownMenuContent align="start" className="min-w-48">
            <DropdownMenuRadioGroup value={current} onValueChange={choose}>
              {options.map(([each, words]) => (
                <DropdownMenuRadioItem key={each} value={each} className="text-sm">
                  {words}
                </DropdownMenuRadioItem>
              ))}
            </DropdownMenuRadioGroup>
          </DropdownMenuContent>
        </DropdownMenu>
        {after}
      </Row>
    );
  }

  if (editor.kind === "people" || editor.kind === "choices") {
    const chosen = new Set(editor.value);
    const fixed = editor.kind === "choices" ? editor.fixed : [];
    const shown = editor.kind === "choices" ? editor.options : editor.people;
    // A value chosen that is no longer on offer (a person who was disabled) stays on the list, so it is shown.
    const offered = new Set(shown.map(([each]) => each));
    const options = [
      ...fixed.map((each): [string, string] => [each, each]),
      ...shown,
      ...editor.value
        .filter((h) => !offered.has(h))
        .map((h): [string, string] => [h, editor.kind === "people" ? `@${h}` : h]),
    ];
    const toggle = (handle: string, on: boolean) => {
      const next = on ? [...editor.value, handle] : editor.value.filter((h) => h !== handle);
      void run(() => editor.onSave(next));
    };
    return (
      <Row nameId={nameId} name={name} refusal={refusal} refusalId={refusalId}>
        <DropdownMenu open={open} onOpenChange={(next) => setOpen(next && !pending)}>
          <DropdownMenuTrigger
            aria-disabled={pending || undefined}
            aria-busy={pending || undefined}
            aria-labelledby={`${nameId} ${valueId}`}
            aria-describedby={described}
            className={VALUE}
          >
            {value}
            <CaretDownIcon aria-hidden="true" className="size-3.5 shrink-0 text-secondary" />
          </DropdownMenuTrigger>
          <DropdownMenuContent align="start" className="min-w-48">
            {options.length === 0 && (
              <div className="px-2 py-1.5 text-sm text-secondary">
                {editor.kind === "people" ? "Nobody to choose" : "Nothing to choose"}
              </div>
            )}
            {options.map(([handle, words]) => (
              <DropdownMenuCheckboxItem
                key={handle}
                checked={chosen.has(handle) || fixed.includes(handle)}
                disabled={fixed.includes(handle)}
                onCheckedChange={(on) => toggle(handle, on)}
                className="text-sm"
              >
                {words}
              </DropdownMenuCheckboxItem>
            ))}
          </DropdownMenuContent>
        </DropdownMenu>
        {after}
      </Row>
    );
  }

  const leave = () => {
    setEditing(false);
    // The focus goes back to the value it was opened from, once the button is drawn again.
    giveBackFocus(button);
  };
  const commit = (typed: string) => {
    leave();
    const text = typed.trim();
    if (editor.kind === "text") void run(() => editor.onSave(text));
    else if (editor.kind === "date") void run(() => editor.onSave(text === "" ? null : text));
    else {
      const number = Number(text.replace(/^#/, ""));
      if (text !== "" && !Number.isFinite(number)) {
        void run(() => Promise.reject(new Error(`"${text}" is not a number.`)));
      } else if (text !== "" && editor.invalid?.(number)) {
        const said = editor.invalid(number) ?? "";
        void run(() => Promise.reject(new Error(said)));
      } else {
        void run(() => editor.onSave(text === "" ? null : number));
      }
    }
  };

  return (
    <Row nameId={nameId} name={name} refusal={refusal} refusalId={refusalId}>
      {editing ? (
        <InlineField
          type={editor.kind}
          initial={typedOf(editor)}
          labelledBy={nameId}
          onCommit={commit}
          onCancel={leave}
        />
      ) : (
        <button
          ref={button}
          type="button"
          aria-disabled={pending || undefined}
          aria-busy={pending || undefined}
          aria-labelledby={`${nameId} ${valueId}`}
          aria-describedby={described}
          onClick={() => {
            if (!pending) setEditing(true);
          }}
          className={VALUE}
        >
          {value}
        </button>
      )}
      {after}
    </Row>
  );
}

/**
 * A property of several values: each one is shown with its own remove button, and "+" opens a field that adds one on
 * Enter. Every add and remove is a save of its own, refused or not under the property.
 */
export function ListProperty({
  name,
  items,
  empty,
  addLabel,
  onAdd,
  onRemove,
}: {
  name: string;
  /** Each value: its key, the words it reads as, and the name of the button that removes it. */
  items: Array<{ key: string; label: string; remove: string }>;
  /** What the property says without any value. */
  empty: string;
  /** The name of the button that opens the field, and of the field: "Add a label". */
  addLabel: string;
  onAdd: (typed: string) => Promise<unknown>;
  onRemove: (key: string) => Promise<unknown>;
}) {
  const id = useId();
  const nameId = `${id}-name`;
  const refusalId = `${id}-refusal`;
  const { pending, refusal, run } = useSave();
  const [adding, setAdding] = useState(false);
  const add = useRef<HTMLButtonElement>(null);
  const described = refusal ? refusalId : undefined;

  const leave = () => {
    setAdding(false);
    giveBackFocus(add);
  };

  return (
    <Row nameId={nameId} name={name} refusal={refusal} refusalId={refusalId}>
      <div className="flex min-w-0 flex-1 flex-wrap items-center gap-1 py-0.5">
        {items.length === 0 && !adding && <span className="text-sm text-secondary">{empty}</span>}
        {items.map((item) => (
          <span
            key={item.key}
            className="inline-flex max-w-full items-center gap-0.5 rounded-full bg-hover py-0.5 pr-0.5 pl-2 text-sm text-text"
          >
            <span className="min-w-0 break-words">{item.label}</span>
            <button
              type="button"
              aria-label={item.remove}
              aria-describedby={described}
              aria-disabled={pending || undefined}
              onClick={() => {
                if (pending) return;
                // The button goes with its value, so the focus goes on to "+" rather than to nowhere.
                add.current?.focus();
                void run(() => onRemove(item.key));
              }}
              className="inline-flex size-6 shrink-0 cursor-pointer items-center justify-center rounded-full bg-transparent text-secondary hover:text-text aria-disabled:cursor-progress aria-disabled:opacity-45"
            >
              <XIcon className="size-3" aria-hidden="true" />
            </button>
          </span>
        ))}
        {adding ? (
          <AddField
            label={addLabel}
            onCommit={(typed) => {
              leave();
              if (typed.trim() !== "") void run(() => onAdd(typed.trim()));
            }}
            onCancel={leave}
          />
        ) : (
          <button
            ref={add}
            type="button"
            aria-label={addLabel}
            title={addLabel}
            aria-describedby={described}
            aria-disabled={pending || undefined}
            onClick={() => {
              if (!pending) setAdding(true);
            }}
            className={SMALL}
          >
            <PlusIcon className="size-3.5" aria-hidden="true" weight="bold" />
          </button>
        )}
      </div>
    </Row>
  );
}

/** The field that adds a value to a list: Enter adds it, Escape or leaving it empty closes it. */
function AddField({
  label,
  onCommit,
  onCancel,
}: {
  label: string;
  onCommit: (typed: string) => void;
  onCancel: () => void;
}) {
  const [typed, setTyped] = useState("");
  const done = useRef(false);
  const finish = (save: boolean) => {
    if (done.current) return;
    done.current = true;
    if (save && typed.trim() !== "") onCommit(typed);
    else onCancel();
  };
  return (
    <input
      // biome-ignore lint/a11y/noAutofocus: the field takes the place of the button a person just pressed
      autoFocus
      aria-label={label}
      value={typed}
      onChange={(event) => setTyped(event.target.value)}
      onBlur={() => finish(true)}
      onKeyDown={(event) => {
        if (event.key === "Enter" && !event.nativeEvent.isComposing) {
          event.preventDefault();
          finish(true);
        } else if (event.key === "Escape") {
          event.preventDefault();
          event.stopPropagation();
          finish(false);
        }
      }}
      className={cn(CONTROL, "min-h-7 w-40 py-0.5 text-sm")}
    />
  );
}
