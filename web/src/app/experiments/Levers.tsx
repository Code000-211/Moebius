/**
 * What the research chosen measured, on the Experiments page: the baseline the levers are compared with, and the table
 * of the levers tried with what each measured and what came of it.
 */
import type { Research } from "../../api/types/improve";
import { Tag } from "../../ui/chat/Tag";
import { Box } from "../../ui/page/Box";
import { CELL, Table } from "../../ui/page/List";
import { Body } from "../../ui/page/PageHeader";
import { change, number, result } from "./model";

/** The number the levers are compared with. */
export function Baseline({ research }: { research: Research }) {
  return (
    <Box title="Baseline">
      {research.baseline === null ? (
        <Body>Not measured yet. The first number the measure gives becomes it.</Body>
      ) : (
        <div className="flex flex-col gap-1">
          <span className="text-2xl font-semibold text-text tabular-nums">
            {number(research.baseline)}
          </span>
        </div>
      )}
    </Box>
  );
}

/** The table of the levers tried: what was changed, what it measured, and what came of it. */
export function Levers({ research }: { research: Research }) {
  const experiments = research.experiments ?? [];
  if (experiments.length === 0) {
    return (
      <Box title="Experiments">
        <Body>No lever has been tried yet.</Body>
      </Box>
    );
  }
  return (
    <Box title="Experiments">
      <Table head={["Lever", "Value", "Change", "Result"]}>
        {experiments.map((experiment) => {
          const shown = result(experiment);
          const moved =
            experiment.value !== null && research.baseline !== null
              ? change(research.baseline, experiment.value)
              : null;
          return (
            <tr key={experiment.id}>
              <td className={CELL}>
                <div className="flex min-w-0 flex-wrap items-baseline gap-x-3">
                  <span className="min-w-0 break-words font-medium text-text">
                    {experiment.title}
                  </span>
                  <code className="min-w-0 break-words font-mono text-sm text-secondary">
                    {experiment.branch}
                  </code>
                </div>
              </td>
              <td className={`${CELL} whitespace-nowrap tabular-nums`}>
                {experiment.value === null ? "—" : number(experiment.value)}
              </td>
              <td className={`${CELL} whitespace-nowrap tabular-nums`}>{moved ?? "—"}</td>
              <td className={CELL}>
                <div className="flex min-w-0 flex-wrap items-center gap-x-3">
                  <Tag tone={shown.tone}>{shown.label}</Tag>
                  {experiment.reason && (
                    <span className="min-w-0 break-words text-sm text-secondary">
                      {experiment.reason}
                    </span>
                  )}
                </div>
              </td>
            </tr>
          );
        })}
      </Table>
    </Box>
  );
}
