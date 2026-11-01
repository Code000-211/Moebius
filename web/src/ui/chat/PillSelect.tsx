/**
 * A choice in the composer: the agent, the model, the reasoning level, the repository. It is a select — the trigger
 * shows what is chosen now and opens a list lined up with itself, Escape closes it, arrows and typing move through it,
 * and a long list (the models) carries a field to filter it.
 *
 * By default the label of what it chooses stands above the current value, so the trigger reads as a field and not as a
 * button of text. `valueOnly` shows the value alone, for a choice whose value already says what it is — a model's
 * name, a reasoning level — where the label would be a second line to read. Either way the label stays as the pill's
 * accessible name, as the word a mouse asks for, and as the head of the list it opens.
 */
import { CaretDownIcon } from "@phosphor-icons/react";
import { type ReactNode, useState } from "react";
import {
  DropdownMenu,
  DropdownMenuContent,
  DropdownMenuLabel,
  DropdownMenuRadioGroup,
  DropdownMenuRadioItem,
  DropdownMenuTrigger,
} from "../components/DropdownMenu";
import { Input } from "../components/Input";
import { cn } from "../lib/cn";

export interface PillOption {
  value: string;
  label: string;
  /** A second line, such as the model an agent runs on. */
  hint?: string;
  /**
   * True when picking this option changes nothing because it is what the agent or the server does anyway. The trigger
   * shows it faded: the words are the state of affairs, not a choice that was made.
   */
  inherited?: boolean;
}

interface PillSelectProps {
  /** What the pill chooses, such as "Model"; it names the pill for a screen reader and heads the list. */
  label: string;
  /** Show the value alone, without the label above it. */
  valueOnly?: boolean;
  value: string | null;
  /** Shown when `value` is null. */
  placeholder: string;
  options: PillOption[];
  onChange: (value: string) => void;
  disabled?: boolean;
  /** Offer a field to filter the options. */
  searchable?: boolean;
  /** Something above the options, such as a note. */
  note?: ReactNode;
  className?: string;
}

export function PillSelect({
  label,
  valueOnly = false,
  value,
  placeholder,
  options,
  onChange,
  disabled,
  searchable,
  note,
  className,
}: PillSelectProps) {
  const [query, setQuery] = useState("");
  const shown = options.filter((option) =>
    `${option.label} ${option.hint ?? ""}`.toLowerCase().includes(query.trim().toLowerCase()),
  );
  const current = options.find((option) => option.value === value);
  // An agent with no model of its own has an empty one rather than a missing one, so what is shown is looked for by
  // meaning, not by presence: an empty label falls back to the value, and an empty value to the placeholder.
  const shownValue = current?.label || value || placeholder;
  const faded = current?.inherited === true;
  const filterName = `Filter ${label.toLowerCase()}s`;
  return (
    <DropdownMenu onOpenChange={() => setQuery("")}>
      <DropdownMenuTrigger
        disabled={disabled}
        aria-label={`${label}: ${shownValue}`}
        title={label}
        className={cn(
          "inline-flex min-h-10 max-w-64 cursor-pointer items-center gap-3 rounded-control bg-sunken px-3 text-left",
          "transition-colors duration-(--motion-routine) ease-out hover:bg-hover",
          "disabled:cursor-not-allowed disabled:opacity-45",
          className,
        )}
      >
        <span className="flex min-w-0 flex-col gap-0">
          {!valueOnly && <span className="text-xs font-semibold text-secondary">{label}</span>}
          <span
            title={shownValue}
            className={cn(
              "break-words text-sm font-normal",
              faded ? "text-secondary" : "text-text",
            )}
          >
            {shownValue}
          </span>
        </span>
        <CaretDownIcon aria-hidden="true" className="size-4 shrink-0 text-secondary" />
      </DropdownMenuTrigger>
      <DropdownMenuContent
        align="start"
        side="top"
        className="w-72 max-w-(--radix-dropdown-menu-content-available-width) min-w-(--radix-dropdown-menu-trigger-width) p-2 shadow-raised"
      >
        <DropdownMenuLabel className="text-xs font-semibold text-secondary">
          {label}
        </DropdownMenuLabel>
        {note && <p className="px-2 pb-2 text-sm text-secondary">{note}</p>}
        {searchable && (
          <Input
            value={query}
            onChange={(event) => setQuery(event.target.value)}
            // Menus choose an item by typing; the field keeps its own keys.
            onKeyDown={(event) => event.stopPropagation()}
            placeholder="Filter by name"
            aria-label={filterName}
            className="mb-2"
          />
        )}
        <DropdownMenuRadioGroup value={value ?? ""} onValueChange={onChange}>
          {shown.map((option) => (
            <DropdownMenuRadioItem
              key={option.value}
              value={option.value}
              className="min-h-10 gap-3 rounded-control py-2"
            >
              <span className="flex min-w-0 flex-col">
                <span
                  className={cn(
                    "break-words text-sm font-normal",
                    option.inherited ? "text-secondary" : "text-text",
                  )}
                >
                  {option.label}
                </span>
                {option.hint && (
                  <span className="break-words font-mono text-xs text-secondary">
                    {option.hint}
                  </span>
                )}
              </span>
            </DropdownMenuRadioItem>
          ))}
        </DropdownMenuRadioGroup>
        {shown.length === 0 && (
          <p className="px-3 py-2 text-sm text-secondary">Nothing matches that name.</p>
        )}
      </DropdownMenuContent>
    </DropdownMenu>
  );
}
