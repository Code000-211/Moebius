/**
 * What the research chosen measured, on the Experiments page: the baseline the levers are compared with, and the table
 * of the levers tried with what each measured and what came of it.
 */
import type { Research } from "../../api/types/improve";
import { Tag } from "../../ui/chat/Tag";
import { plural } from "../../ui/lib/format";
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
        <div className="flex flex-col gap-2">
          <span className="text-2xl font-semibold text-text tabular-nums">
            {number(research.baseline)}
          </span>
          <p className="m-0 max-w-prose text-sm text-secondary">
            The number the measure gave with nothing changed. Each lever is judged against it.
          </p>
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
      <Box title="Experiments" aside="One lever each">
        <Body>No lever has been tried yet.</Body>
      </Box>
    );
  }
  return (
    <Box
      title="Experiments"
      aside={
        <span className="tabular-nums">{plural(experiments.length, "lever", "levers")} tried</span>
      }
    >
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
                <div className="flex min-w-0 flex-col gap-2">
                  <span className="min-w-0 break-words font-semibold text-text">
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
                <div className="flex min-w-0 flex-col gap-2">
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
