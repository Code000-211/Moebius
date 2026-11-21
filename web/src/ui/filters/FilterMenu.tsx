/**
 * The quiet "Filter" text button of a page's header, for a list that has no bar under its header: it reads "Filter"
 * and, once something is set, "Filter · n" in the accent. It opens a small menu with a submenu of values for each
 * field and, while any is set, "Clear filters". What is set is the page's to keep (the address, by `model.ts`); the
 * menu only says what changed.
 */
import { Button } from "../components/Button";
import {
  DropdownMenu,
  DropdownMenuContent,
  DropdownMenuItem,
  DropdownMenuRadioGroup,
  DropdownMenuRadioItem,
  DropdownMenuSub,
  DropdownMenuSubContent,
  DropdownMenuSubTrigger,
  DropdownMenuTrigger,
} from "../components/DropdownMenu";
import { cn } from "../lib/cn";
import { activeFilters, type FilterField } from "./model";

export function FilterMenu({
  fields,
  values,
  onFilter,
  onClear,
}: {
  fields: FilterField[];
  /** The value of each field by its key; empty for one that is not set. */
  values: Readonly<Record<string, string>>;
  /** Sets a field to a value, or removes it with the empty string. */
  onFilter: (key: string, value: string) => void;
  onClear: () => void;
}) {
  const set = activeFilters(fields, values).length;
  const offered = fields.filter((field) => field.options.length > 0);
  return (
    <DropdownMenu>
      <DropdownMenuTrigger asChild>
        <Button variant="ghost" size="sm" className={cn(set > 0 && "text-accent-text")}>
          {set > 0 ? `Filter · ${set}` : "Filter"}
        </Button>
      </DropdownMenuTrigger>
      <DropdownMenuContent align="end" aria-label="Filter by">
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
        {set > 0 && <DropdownMenuItem onSelect={onClear}>Clear filters</DropdownMenuItem>}
      </DropdownMenuContent>
    </DropdownMenu>
  );
}
