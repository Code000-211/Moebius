/**
 * The boxes under the focal figure of the Usage page: tokens by model, where the model time goes, the longest tool
 * waits and the usage by agent. Figures are body or metadata text with `tabular-nums` so they line
 * up as they change; every chart is a category colour, so none of them borrows a status hue.
 */
import type { ReactNode } from "react";
import type { UsageReport } from "../../api/types/usage";
import { cn } from "../../ui/lib/cn";
import { noun, plural } from "../../ui/lib/format";
import { Box } from "../../ui/page/Box";
import { CELL, Table } from "../../ui/page/List";
import { duration, grouped, share, short } from "./figures";
import { doings, modelColours } from "./model";

/**
 * What a box says when the window has nothing to show. The first box says once what fills the page; the boxes under it
 * name the state only, so the screen does not read the same sentence three times.
 */
const NOTHING = "Nothing ran in this window.";
const NOTHING_ONCE = `${NOTHING} Figures appear once a card or a chat makes a model call.`;

/** The six category colours, in the order `modelColours` hands them to the models. */
const CAT = ["bg-cat-1", "bg-cat-2", "bg-cat-3", "bg-cat-4", "bg-cat-5", "bg-cat-6"];

/** A tool's name, which is an identifier and so keeps the code font. */
const code = (name: string) => <span className="font-mono text-sm">{name}</span>;

/** A cell of a table that holds words. */
const word = (content: ReactNode) => <td className={CELL}>{content}</td>;

/** A cell of a table that holds a figure: right aligned, with the digits lined up. */
const figure = (content: ReactNode) => (
  <td className={cn(CELL, "text-right tabular-nums")}>{content}</td>
);

/** The stacked bar of the models' shares; the table under it names every share in words. */
export function ModelsBox({ report, span }: { report: UsageReport; span: string }) {
  const colour = modelColours(report);
  const total = report.models.reduce((sum, model) => sum + model.tokens, 0);
  return (
    <Box
      title="Tokens by model"
      aside={`${short(total)} ${span} across ${plural(report.models.length, "model", "models")}`}
    >
      {report.models.length === 0 ? (
        <p className="m-0 text-base text-secondary">{NOTHING_ONCE}</p>
      ) : (
        <div className="flex flex-col gap-3">
          <div aria-hidden="true" className="flex h-2 gap-px overflow-hidden rounded-full bg-track">
            {report.models.map((model) => (
              <span
                key={model.model}
                className={cn("h-full", CAT[colour(model.model) - 1])}
                style={{
                  flex: `${Math.max(1, Math.round((Math.max(0, model.tokens) * 1000) / Math.max(1, total)))} 1 0`,
                }}
              />
            ))}
          </div>
          <Table head={["Model", "Tokens", "Share", "Calls", "Tokens per call", "From cache"]}>
            {report.models.map((model) => (
              <tr key={model.model}>
                {word(
                  <span className="flex items-center gap-2">
                    <span
                      aria-hidden="true"
                      className={cn("size-3 shrink-0 rounded-small", CAT[colour(model.model) - 1])}
                    />
                    <span className="break-words">{model.model}</span>
                  </span>,
                )}
                {figure(short(model.tokens))}
                {figure(share(model.tokens, total))}
                {figure(grouped(model.calls))}
                {figure(grouped(Math.floor(model.tokens / Math.max(1, model.calls))))}
                {figure(share(model.cached_tokens, model.input_tokens))}
              </tr>
            ))}
          </Table>
        </div>
      )}
    </Box>
  );
}

/** What the model did with its time, as a bar for each activity against the one that took longest. */
export function TimeBox({ report, span }: { report: UsageReport; span: string }) {
  const list = doings(report.activities);
  const longest = Math.max(1, ...list.map((each) => each.modelMs));
  return (
    <Box
      title="Where the model time goes"
      aside={`${duration(report.totals.model_ms / 1000)} ${span}`}
    >
      {list.length === 0 ? (
        <p className="m-0 text-base text-secondary">{NOTHING}</p>
      ) : (
        // biome-ignore lint/a11y/noRedundantRoles: preflight sets list-style: none, and WebKit then drops a list with no marker from the accessibility tree entirely
        <ul role="list" className="m-0 flex list-none flex-col gap-3 p-0">
          {list.map((each) => (
            <li key={each.name} className="flex flex-col gap-2">
              <div className="flex flex-wrap items-baseline justify-between gap-3">
                <span className="break-words text-base text-text">
                  {each.tool ? code(each.name) : each.name}
                  <span className="text-sm text-secondary">
                    {` · ${grouped(each.calls)} ${noun(each.calls, "call", "calls")}`}
                  </span>
                </span>
                <span className="text-sm tabular-nums text-secondary">
                  {duration(each.modelMs / 1000)}
                </span>
              </div>
              <div aria-hidden="true" className="h-2 overflow-hidden rounded-full bg-track">
                <span
                  className="block h-full rounded-full bg-cat-6"
                  style={{ width: `${Math.max(2, (each.modelMs * 100) / longest)}%` }}
                />
              </div>
            </li>
          ))}
        </ul>
      )}
    </Box>
  );
}

/** The tools whose calls made runs wait longest. */
export function WaitsBox({ report }: { report: UsageReport }) {
  return (
    <Box title="Longest tool waits">
      {report.tool_waits.length === 0 ? (
        <p className="m-0 text-base text-secondary">
          No tool call waited long enough to be worth naming.
        </p>
      ) : (
        <Table head={["Tool call", "Calls", "Waited"]}>
          {report.tool_waits.map((wait) => (
            <tr key={wait.tool}>
              {word(code(wait.tool))}
              {figure(grouped(wait.calls))}
              {figure(duration(wait.seconds))}
            </tr>
          ))}
        </Table>
      )}
    </Box>
  );
}

/** What each agent used, so a person can see where the tokens came from. */
export function AgentsBox({ report }: { report: UsageReport }) {
  return (
    <Box
      title="By agent"
      aside={report.agents.length > 0 ? plural(report.agents.length, "agent", "agents") : undefined}
    >
      {report.agents.length === 0 ? (
        <p className="m-0 text-base text-secondary">{NOTHING}</p>
      ) : (
        <Table head={["Agent", "Calls", "Tokens", "From cache", "Model time"]}>
          {report.agents.map((agent) => (
            <tr key={agent.agent}>
              {word(agent.agent)}
              {figure(grouped(agent.calls))}
              {figure(short(agent.tokens))}
              {figure(share(agent.cached_tokens, agent.input_tokens))}
              {figure(duration(agent.model_ms / 1000))}
            </tr>
          ))}
        </Table>
      )}
    </Box>
  );
}
