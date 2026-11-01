/**
 * The filters over a board: repository (opens that board), column, agent and status. Each is a native select, so the
 * keyboard and screen readers work as they do everywhere; "Clear" shows while any filter is set.
 */
import { FunnelIcon } from "@phosphor-icons/react";
import type { ReactNode } from "react";
import type { Repository } from "../../api/types/board";
import { Button } from "../../ui/components/Button";
import { type ColumnDef, type Filters, hasFilters, STATUS_LABELS } from "./model";

function FilterSelect({
  label,
  value,
  onChange,
  children,
}: {
  label: string;
  value: string;
  onChange: (value: string) => void;
  children: ReactNode;
}) {
  return (
    <label className="flex items-center gap-half text-xs text-low">
      {label}
      <select
        value={value}
        onChange={(e) => onChange(e.target.value)}
        className="h-7 rounded-md border border-line-strong bg-recessed px-base text-xs text-high focus-visible:border-accent focus-visible:outline-none"
      >
        {children}
      </select>
    </label>
  );
}

export interface FilterBarProps {
  repositories: Repository[];
  repository: number;
  onRepository: (id: number) => void;
  columns: ColumnDef[];
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
    <search
      aria-label="Filter the cards"
      className="mb-[12px] flex flex-wrap items-center gap-base"
    >
      <FunnelIcon className="size-icon-sm text-low" weight="bold" aria-hidden="true" />
      <FilterSelect
        label="Repository"
        value={String(repository)}
        onChange={(value) => onRepository(Number(value))}
      >
        {repositories.map((r) => (
          <option key={r.id} value={r.id}>
            {r.name}
          </option>
        ))}
      </FilterSelect>
      <FilterSelect
        label="Column"
        value={filters.column}
        onChange={(column) => onChange({ ...filters, column })}
      >
        <option value="">All columns</option>
        {columns.map((c) => (
          <option key={c.name} value={c.name}>
            {c.name}
          </option>
        ))}
      </FilterSelect>
      <FilterSelect
        label="Agent"
        value={filters.agent}
        onChange={(agent) => onChange({ ...filters, agent })}
      >
        <option value="">All agents</option>
        {agents.map((a) => (
          <option key={a} value={a}>
            {a}
          </option>
        ))}
      </FilterSelect>
      <FilterSelect
        label="Status"
        value={filters.status}
        onChange={(status) => onChange({ ...filters, status })}
      >
        <option value="">Any status</option>
        {Object.entries(STATUS_LABELS).map(([value, label]) => (
          <option key={value} value={value}>
            {label}
          </option>
        ))}
      </FilterSelect>
      {hasFilters(filters) && (
        <Button
          variant="ghost"
          size="xs"
          onClick={() => onChange({ column: "", agent: "", status: "" })}
        >
          Clear
        </Button>
      )}
    </search>
  );
}
