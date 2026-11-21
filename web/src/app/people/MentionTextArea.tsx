/**
 * A text area that completes `@handles`: typing `@` and a few letters lists the people of the organisation who are
 * not disabled (`mentions.ts`), arrows move in the list, Enter or Tab writes the handle and Escape closes it. It is
 * the combobox of the ARIA pattern: the text area keeps the focus and names the highlighted option. A mention tells
 * the person (ADR 0018), which is why it is offered where a comment is written.
 */
import { useQuery } from "@tanstack/react-query";
import { type ComponentProps, useId, useRef, useState } from "react";
import { people as loadPeople } from "../../api/calls/people";
import { keys } from "../../api/keys";
import { cn } from "../../ui/lib/cn";
import { TextArea } from "../../ui/page/Field";
import { completed, offered, type Typing, typing } from "./mentions";

type Props = Omit<ComponentProps<typeof TextArea>, "value" | "onChange"> & {
  value: string;
  onValueChange: (value: string) => void;
};

export function MentionTextArea({ value, onValueChange, onKeyDown, onBlur, ...props }: Props) {
  const list = useId();
  const area = useRef<HTMLTextAreaElement>(null);
  const people = useQuery({ queryKey: keys.people, queryFn: loadPeople });
  const [caret, setCaret] = useState(0);
  const [choice, setChoice] = useState(0);
  const [closed, setClosed] = useState(false);
  const now: Typing | null = closed ? null : typing(value, caret);
  const options = now === null ? [] : offered(people.data ?? [], now.query);
  const open = now !== null && options.length > 0;
  const active = Math.min(choice, Math.max(options.length - 1, 0));

  const pick = (handle: string) => {
    if (now === null) return;
    const next = completed(value, now.at, caret, handle);
    onValueChange(next.text);
    setCaret(next.caret);
    setChoice(0);
    // The caret moves once React has written the text.
    requestAnimationFrame(() => area.current?.setSelectionRange(next.caret, next.caret));
  };

  return (
    <div className="relative">
      <TextArea
        {...props}
        ref={area}
        value={value}
        role="combobox"
        aria-expanded={open}
        aria-autocomplete="list"
        aria-controls={open ? list : undefined}
        aria-activedescendant={open ? `${list}-${active}` : undefined}
        onChange={(e) => {
          onValueChange(e.target.value);
          setCaret(e.target.selectionStart);
          setChoice(0);
          setClosed(false);
        }}
        onSelect={(e) => setCaret(e.currentTarget.selectionStart)}
        onBlur={(e) => {
          setClosed(true);
          onBlur?.(e);
        }}
        onKeyDown={(e) => {
          onKeyDown?.(e);
          if (!open || e.defaultPrevented) return;
          if (e.key === "ArrowDown" || e.key === "ArrowUp") {
            e.preventDefault();
            const step = e.key === "ArrowDown" ? 1 : options.length - 1;
            setChoice((active + step) % options.length);
          } else if (e.key === "Enter" || e.key === "Tab") {
            e.preventDefault();
            const chosen = options[active];
            if (chosen) pick(chosen.handle);
          } else if (e.key === "Escape") {
            e.preventDefault();
            setClosed(true);
          }
        }}
      />
      {open && (
        <div
          id={list}
          role="listbox"
          aria-label="People"
          className="absolute left-0 z-10 mt-1 w-64 max-w-full overflow-hidden rounded-control bg-panel py-1 shadow-raised outline outline-1 -outline-offset-1 outline-hairline-strong"
        >
          {options.map((person, index) => (
            // biome-ignore lint/a11y/useKeyWithClickEvents: the text area has the focus and takes the keys
            <div
              key={person.id}
              id={`${list}-${index}`}
              role="option"
              tabIndex={-1}
              aria-selected={index === active}
              // The mouse must not take the focus from the text area, or the list would close before the click.
              onMouseDown={(e) => e.preventDefault()}
              onClick={() => pick(person.handle)}
              className={cn(
                "flex min-h-8 cursor-pointer items-center gap-2 px-3 text-sm text-text",
                index === active ? "bg-selected" : "hover:bg-hover",
              )}
            >
              <span className="font-medium">@{person.handle}</span>
              {person.name !== "" && <span className="truncate text-secondary">{person.name}</span>}
            </div>
          ))}
        </div>
      )}
    </div>
  );
}
