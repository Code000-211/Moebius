/**
 * A pill in the composer that opens a list to choose from: the agent, the model, the reasoning level, the repository.
 * A simple stand-in for Vibe Kanban's `ModelSelectorPopover`, which is made for providers and accordions that Moebius's
 * models do not have. A long list (the models) has a field to filter it.
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
import { cn } from "../lib/cn";

export interface PillOption {
  value: string;
  label: string;
  /** A second line, such as the model an agent runs on. */
  hint?: string;
}

interface PillSelectProps {
  /** What the pill chooses, such as "Model"; it names the pill for a screen reader and heads the list. */
  label: string;
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
  const current = options.find((option) => option.value === value)?.label ?? value ?? placeholder;
  return (
    <DropdownMenu onOpenChange={() => setQuery("")}>
      <DropdownMenuTrigger
        disabled={disabled}
        aria-label={`${label}: ${current}`}
        title={label}
        className={cn(
          "inline-flex max-w-[16rem] items-center gap-half rounded-full border border-line-strong bg-card px-plusfifty py-half text-xs text-normal outline-none transition-colors hover:bg-card-hover focus-visible:ring-1 focus-visible:ring-accent/60 disabled:cursor-not-allowed disabled:opacity-50",
          value === null && "text-low",
          className,
        )}
      >
        <span className="truncate">{current}</span>
        <CaretDownIcon className="size-icon-xs shrink-0" />
      </DropdownMenuTrigger>
      <DropdownMenuContent
        align="start"
        side="top"
        className="max-h-80 min-w-[12rem] max-w-[22rem]"
      >
        <DropdownMenuLabel className="font-mono text-xs font-normal text-low">
          {label}
        </DropdownMenuLabel>
        {note && <div className="px-2 pb-1 text-xs text-low">{note}</div>}
        {searchable && (
          <input
            value={query}
            onChange={(event) => setQuery(event.target.value)}
            // Menus choose an item by typing; the field keeps its own keys.
            onKeyDown={(event) => event.stopPropagation()}
            placeholder={`Filter ${label.toLowerCase()}s`}
            aria-label={`Filter ${label.toLowerCase()}s`}
            className="mx-1 mb-1 w-[calc(100%-0.5rem)] rounded-sm border bg-recessed px-base py-half text-xs text-high outline-none"
          />
        )}
        <DropdownMenuRadioGroup value={value ?? ""} onValueChange={onChange}>
          {shown.map((option) => (
            <DropdownMenuRadioItem key={option.value} value={option.value}>
              <span className="flex min-w-0 flex-col">
                <span className="truncate">{option.label}</span>
                {option.hint && (
                  <span className="truncate font-mono text-xs text-low">{option.hint}</span>
                )}
              </span>
            </DropdownMenuRadioItem>
          ))}
        </DropdownMenuRadioGroup>
        {shown.length === 0 && <div className="px-2 py-1.5 text-xs text-low">Nothing matches.</div>}
      </DropdownMenuContent>
    </DropdownMenu>
  );
}
