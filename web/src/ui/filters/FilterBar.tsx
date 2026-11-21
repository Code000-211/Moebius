/**
 * The one filter bar every page of things shares (the board first): a chip for each filter that is set, "field is
 * value", that opens the values to change it and has a button to remove it; "+ Filter" with a menu of the fields and,
 * beside each, its values; Display for the layout, the grouping, the sorting and the fields a card shows; and "Save
 * view" once something is set. Every part is a menu of the app's own, so the keyboard works as it does everywhere: the
 * arrows move, the right arrow opens a field's values, Enter chooses, Escape closes. What is set is the page's to keep
 * (the address, by `model.ts`); the bar only says what changed.
 */
import { BookmarkSimpleIcon, PlusIcon, SlidersHorizontalIcon, XIcon } from "@phosphor-icons/react";
import { memo, type ReactNode } from "react";
import { Button } from "../components/Button";
import {
  DropdownMenu,
  DropdownMenuCheckboxItem,
  DropdownMenuContent,
  DropdownMenuLabel,
  DropdownMenuRadioGroup,
  DropdownMenuRadioItem,
  DropdownMenuSub,
  DropdownMenuSubContent,
  DropdownMenuSubTrigger,
  DropdownMenuTrigger,
} from "../components/DropdownMenu";
import {
  type ActiveFilter,
  activeFilters,
  type Display,
  type DisplayChoices,
  type FilterField,
  hasDisplay,
  type Layout,
  toggled,
} from "./model";

/** What Display offers on a page and what it holds now. */
export interface DisplayProps {
  choices: DisplayChoices;
  value: Display;
  onChange: (display: Display) => void;
}

export interface FilterBarProps {
  /** The name of the region, said to a screen reader: "Filter the cards". */
  label: string;
  /** What stands before the chips, such as the choice of repository. */
  leading?: ReactNode;
  fields: FilterField[];
  /** The value of each field by its key; empty for one that is not set. */
  values: Readonly<Record<string, string>>;
  /** Sets a field to a value, or removes it with the empty string. */
  onFilter: (key: string, value: string) => void;
  onClear: () => void;
  display?: DisplayProps;
  /** Offered once a filter or a Display choice is set. */
  onSaveView?: () => void;
}

const LAYOUTS: Array<[Layout, string]> = [
  ["board", "Board"],
  ["list", "List"],
];

/** One filter that is set: the chip that opens its values, and the button that removes it. */
export function FilterChip({
  active,
  onChange,
}: {
  active: ActiveFilter;
  onChange: (value: string) => void;
}) {
  const { field, value, words } = active;
  return (
    <div className="inline-flex min-h-7 items-center rounded-control bg-selected text-sm font-medium text-text">
      <DropdownMenu>
        <DropdownMenuTrigger
          className="min-h-7 cursor-pointer rounded-l-control px-2 hover:bg-hover"
          title={`${field.label} is ${words}`}
        >
          {field.label} <span className="font-normal text-secondary">is</span> {words}
        </DropdownMenuTrigger>
        <DropdownMenuContent align="start">
          <DropdownMenuRadioGroup value={value} onValueChange={onChange}>
            {field.options.map(([each, name]) => (
              <DropdownMenuRadioItem key={each} value={each} className="text-sm">
                {name}
              </DropdownMenuRadioItem>
            ))}
          </DropdownMenuRadioGroup>
        </DropdownMenuContent>
      </DropdownMenu>
      <button
        type="button"
        aria-label={`Remove filter ${field.label} is ${words}`}
        onClick={() => onChange("")}
        className="inline-flex min-h-7 cursor-pointer items-center rounded-r-control px-2 text-secondary hover:bg-hover hover:text-text"
      >
        <XIcon className="size-3" weight="bold" aria-hidden="true" />
      </button>
    </div>
  );
}

/**
 * Display's menu. `plain` draws the trigger as the quiet word of a page header, without its icon; `extra` is more items
 * at the foot of the menu, for what a page keeps there besides the choices.
 */
export function DisplayMenu({
  choices,
  value,
  onChange,
  plain = false,
  extra,
}: DisplayProps & { plain?: boolean; extra?: ReactNode }) {
  return (
    <DropdownMenu>
      <DropdownMenuTrigger asChild>
        <Button variant="ghost" size="sm" className={plain ? QUIET : undefined}>
          {!plain && <SlidersHorizontalIcon className="size-4" aria-hidden="true" />}
          Display
        </Button>
      </DropdownMenuTrigger>
      <DropdownMenuContent align="end" aria-label="Display">
        {(choices.layouts ?? LAYOUTS).length > 0 && (
          <>
            <DropdownMenuLabel>Layout</DropdownMenuLabel>
            <DropdownMenuRadioGroup
              value={value.layout}
              onValueChange={(layout) => onChange({ ...value, layout: layout as Layout })}
            >
              {(choices.layouts ?? LAYOUTS).map(([each, name]) => (
                <DropdownMenuRadioItem key={each} value={each}>
                  {name}
                </DropdownMenuRadioItem>
              ))}
            </DropdownMenuRadioGroup>
          </>
        )}
        {value.layout === "list" && choices.groups.length > 0 && (
          <>
            <DropdownMenuLabel>Grouping</DropdownMenuLabel>
            <DropdownMenuRadioGroup
              value={value.group}
              onValueChange={(group) => onChange({ ...value, group })}
            >
              {choices.groups.map(([each, name]) => (
                <DropdownMenuRadioItem key={each} value={each}>
                  {name}
                </DropdownMenuRadioItem>
              ))}
            </DropdownMenuRadioGroup>
          </>
        )}
        <DropdownMenuLabel>Sorting</DropdownMenuLabel>
        <DropdownMenuRadioGroup
          value={value.sort}
          onValueChange={(sort) => onChange({ ...value, sort })}
        >
          {choices.sorts.map(([each, name]) => (
            <DropdownMenuRadioItem key={each} value={each}>
              {name}
            </DropdownMenuRadioItem>
          ))}
        </DropdownMenuRadioGroup>
        {choices.fields.length > 0 && <DropdownMenuLabel>Fields shown</DropdownMenuLabel>}
        {choices.fields.map(([key, name]) => (
          <DropdownMenuCheckboxItem
            key={key}
            checked={!value.hidden.includes(key)}
            // Several fields are set in one visit, so the menu stays open.
            onSelect={(event) => event.preventDefault()}
            onCheckedChange={(shown) =>
              onChange({ ...value, hidden: toggled(value.hidden, key, shown) })
            }
          >
            {name}
          </DropdownMenuCheckboxItem>
        ))}
        {extra}
      </DropdownMenuContent>
    </DropdownMenu>
  );
}

/** The quiet word of a page header: muted, no icon, the text colour on hover. */
const QUIET = "text-muted hover:text-text";

/**
 * The Filter menu: a submenu of values for each field that has some. `before` is submenus of the page's own, ahead of the
 * fields; `plain` draws the trigger as the quiet word of a page header.
 */
export function FilterMenu({
  fields,
  values,
  onFilter,
  before,
  plain = false,
}: {
  fields: FilterField[];
  values: Readonly<Record<string, string>>;
  onFilter: (key: string, value: string) => void;
  before?: ReactNode;
  plain?: boolean;
}) {
  const offered = fields.filter((field) => field.options.length > 0);
  return (
    <DropdownMenu>
      <DropdownMenuTrigger asChild>
        <Button variant="ghost" size="sm" className={plain ? QUIET : undefined}>
          {!plain && <PlusIcon className="size-4" aria-hidden="true" />}
          Filter
        </Button>
      </DropdownMenuTrigger>
      <DropdownMenuContent align={plain ? "end" : "start"} aria-label="Filter by">
        {before}
        {offered.map((field) => (
          <DropdownMenuSub key={field.key}>
            <DropdownMenuSubTrigger>{field.label}</DropdownMenuSubTrigger>
            <DropdownMenuSubContent aria-label={field.label}>
              <DropdownMenuRadioGroup
                value={values[field.key] ?? ""}
                onValueChange={(value) => onFilter(field.key, value)}
              >
                {field.options.map(([each, name]) => (
                  <DropdownMenuRadioItem key={each} value={each} className="text-sm">
                    {name}
                  </DropdownMenuRadioItem>
                ))}
              </DropdownMenuRadioGroup>
            </DropdownMenuSubContent>
          </DropdownMenuSub>
        ))}
      </DropdownMenuContent>
    </DropdownMenu>
  );
}

export const FilterBar = memo(function FilterBar({
  label,
  leading,
  fields,
  values,
  onFilter,
  onClear,
  display,
  onSaveView,
}: FilterBarProps) {
  const active = activeFilters(fields, values);
  const anySet = active.length > 0 || (display !== undefined && hasDisplay(display.value));
  return (
    <search aria-label={label} className="mb-4 flex flex-wrap items-center gap-2">
      {leading}
      {active.map((each) => (
        <FilterChip
          key={each.field.key}
          active={each}
          onChange={(value) => onFilter(each.field.key, value)}
        />
      ))}
      {fields.length > 0 && <FilterMenu fields={fields} values={values} onFilter={onFilter} />}
      {active.length > 0 && (
        <Button variant="ghost" size="sm" onClick={onClear}>
          Clear filters
        </Button>
      )}
      <div className="ml-auto flex items-center gap-2">
        {onSaveView && anySet && (
          <Button variant="ghost" size="sm" onClick={onSaveView}>
            <BookmarkSimpleIcon className="size-4" aria-hidden="true" />
            Save view
          </Button>
        )}
        {display && <DisplayMenu {...display} />}
      </div>
    </search>
  );
});
