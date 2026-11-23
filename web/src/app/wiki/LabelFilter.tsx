/**
 * The chips above the wiki's tree: "All" and every label in use in the pages shown. One or more labels chosen narrow
 * the tree to the pages that carry all of them; "All" clears the choice.
 */
import { Chip } from "../../ui/page/Chip";

export function LabelFilter({
  labels,
  chosen,
  onChange,
}: {
  labels: ReadonlyArray<{ label: string; count: number }>;
  chosen: readonly string[];
  onChange: (chosen: string[]) => void;
}) {
  if (labels.length === 0) return null;
  return (
    <fieldset
      aria-label="Filter by label"
      className="m-0 flex min-w-0 flex-wrap gap-1.5 border-0 p-0"
    >
      <Chip selected={chosen.length === 0} onClick={() => onChange([])}>
        All
      </Chip>
      {labels.map(({ label, count }) => (
        <Chip
          key={label}
          selected={chosen.includes(label)}
          onClick={() =>
            onChange(
              chosen.includes(label) ? chosen.filter((each) => each !== label) : [...chosen, label],
            )
          }
        >
          {label}
          <span className="tabular-nums text-muted">{count}</span>
        </Chip>
      ))}
    </fieldset>
  );
}
