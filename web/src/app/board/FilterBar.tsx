/**
 * The filters over a board: repository (opens that board), column, agent and status. Each is a labelled native select,
 * so the keyboard and screen readers work as they do everywhere and the value chosen is always in full; "Clear
 * filters" shows while any filter is set. The fields are 40px and sit on one row, grouped by the space between them.
 */
import type { Repository } from "../../api/types/board";
import { Button } from "../../ui/components/Button";
import { Field, Select } from "../../ui/page/Field";
import { type Filters, hasFilters, STATUS_LABELS } from "./model";

function Filter({
  label,
  value,
  options,
  onChange,
}: {
  label: string;
  value: string;
  /** Each choice is its value, or a `[value, label]` pair. The field is as wide as the widest of them. */
  options: Array<string | [string, string]>;
  onChange: (value: string) => void;
}) {
  return (
    <Field label={label} className="min-w-44">
      {(id) => (
        <Select id={id} value={value} options={options} onChange={onChange} className="w-auto" />
      )}
    </Field>
  );
}

export interface FilterBarProps {
  repositories: Repository[];
  repository: number;
  onRepository: (id: number) => void;
  /** The names of the columns, in the order of the board. */
  columns: string[];
  agents: string[];
  filters: Filters;
  onChange: (filters: Filters) => void;
}

export function FilterBar({
  repositories,
  repository,
  onRepository,
  columns,
  agents,
  filters,
  onChange,
}: FilterBarProps) {
  return (
    <search aria-label="Filter the cards" className="mb-4 flex flex-wrap items-end gap-3">
      <Filter
        label="Repository"
        value={String(repository)}
        options={repositories.map((r): [string, string] => [String(r.id), r.name])}
        onChange={(value) => onRepository(Number(value))}
      />
      <Filter
        label="Column"
        value={filters.column}
        options={[["", "All columns"], ...columns.map((name): [string, string] => [name, name])]}
        onChange={(column) => onChange({ ...filters, column })}
      />
      <Filter
        label="Agent"
        value={filters.agent}
        options={[["", "All agents"], ...agents.map((a): [string, string] => [a, a])]}
        onChange={(agent) => onChange({ ...filters, agent })}
      />
      <Filter
        label="Status"
        value={filters.status}
        options={[["", "Any status"], ...Object.entries(STATUS_LABELS)]}
        onChange={(status) => onChange({ ...filters, status })}
      />
      {hasFilters(filters) && (
        <Button
          variant="ghost"
          size="sm"
          onClick={() => onChange({ column: "", agent: "", status: "" })}
        >
          Clear filters
        </Button>
      )}
    </search>
  );
}
