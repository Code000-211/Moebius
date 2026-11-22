/**
 * The Extensions tab of an agent's page: what the agent is given besides its skills, in the page's full width. "Installed"
 * lists the Pi packages that name the agent (version, what kind of package, Remove) and the extension files of the
 * repository it names by path; "Add from pi.dev" searches the catalog (a search field that waits for the typing to settle,
 * a filter by kind) and each result has an Add. Adding and removing a package is a proposal, a change of the harness
 * configuration repository that a person merges, so what a request told is shown at the top of the tab with the link to
 * that change in the Inbox, and the package list is read again afterwards.
 */
import { MagnifyingGlassIcon } from "@phosphor-icons/react";
import { keepPreviousData, useQuery } from "@tanstack/react-query";
import { useMemo, useState } from "react";
import { snapshot as readSnapshot } from "../../api/calls/board";
import { installedPi, proposePi, removePi, searchPi, updatePi } from "../../api/calls/packages";
import { keys } from "../../api/keys";
import type { PiHit, PiInstalled, PiResources } from "../../api/types/packages";
import {
  changelogs,
  downloads,
  kindLabel,
  PI_KINDS,
  piSource,
  tellOffered,
  tellUpdates,
  waitingPiUpdates,
} from "../../packages/model";
import { PlainList } from "../../packages/parts";
import { type Report, ReportView, reported } from "../../packages/Report";
import { useActions } from "../../packages/useActions";
import { Tag } from "../../ui/chat/Tag";
import { Button } from "../../ui/components/Button";
import { Input } from "../../ui/components/Input";
import { useSettled } from "../../ui/lib/useSettled";
import { Box } from "../../ui/page/Box";
import { Chip } from "../../ui/page/Chip";
import { Empty } from "../../ui/page/Empty";
import { ReadFailed } from "../../ui/page/ReadFailed";
import { SkeletonList } from "../../ui/page/Skeleton";
import { Paths } from "./AgentParts";
import type { AgentForm } from "./model";

/** The kinds a package holds, as the chips of its row. */
export function kindsHeld(resources: PiResources): string[] {
  const held: string[] = [];
  if (resources.extensions.length > 0) held.push("extension");
  if (resources.skills.length > 0) held.push("skill");
  if (resources.themes.length > 0) held.push("theme");
  if (resources.prompts.length > 0) held.push("prompt");
  return held;
}

/** The Pi packages that name the agent `name`. */
export const packagesOf = (all: PiInstalled[], name: string): PiInstalled[] =>
  all.filter((each) => each.agents.includes(name));

function Row({
  name,
  line,
  kinds,
  button,
  testId,
}: {
  name: string;
  line: string;
  kinds: string[];
  button: React.ReactNode;
  testId: string;
}) {
  return (
    <li
      data-testid={testId}
      className="flex min-h-12 min-w-0 items-center gap-4 border-b border-hairline py-2"
    >
      <div className="flex min-w-0 flex-1 flex-col gap-0.5">
        <span className="min-w-0 break-all font-mono text-sm font-semibold text-text">{name}</span>
        {line && <span className="min-w-0 break-words text-sm text-muted">{line}</span>}
      </div>
      <div className="flex shrink-0 flex-wrap items-center justify-end gap-2">
        {kinds.map((kind) => (
          <Tag key={kind}>{kindLabel(kind)}</Tag>
        ))}
        {button}
      </div>
    </li>
  );
}

export function AgentExtensions({
  form,
  change,
}: {
  form: AgentForm;
  change: (patch: Partial<AgentForm>) => void;
}) {
  const agent = form.exists ? form.name : null;
  const [report, setReport] = useState<Report | null>(null);
  const actions = useActions(setReport);
  const [query, setQuery] = useState("");
  const [kind, setKind] = useState<string | null>(null);
  const settled = useSettled(query).trim();
  const installed = useQuery({ queryKey: keys.piInstalled, queryFn: installedPi, retry: false });
  const found = useQuery({
    queryKey: keys.piSearch(settled, kind),
    queryFn: () => searchPi(settled, kind),
    placeholderData: keepPreviousData,
  });
  const board = useQuery({ queryKey: keys.snapshot, queryFn: () => readSnapshot() });
  // The nightly check's newer versions, which wait in the Inbox for a person to merge them.
  const waiting = useMemo(() => waitingPiUpdates(board.data?.inbox ?? []), [board.data]);
  const mine = packagesOf(installed.data ?? [], form.name);
  const named = new Set(mine.map((each) => each.name));
  const results = (found.data ?? []).filter((hit) => kind === null || hit.kinds.includes(kind));

  return (
    <div className="flex min-w-0 flex-col gap-6" data-testid="agent-extensions">
      {report && <ReportView report={report} onLeave={() => {}} />}
      <Box title="Installed" aside="Pi packages" className="flex flex-col gap-2 [&>header]:mb-0">
        {installed.isPending ? (
          <SkeletonList rows={2} message="Reading what is installed" />
        ) : installed.isError ? (
          <ReadFailed
            error={installed.error}
            fallback="What is installed could not be read. Try again."
            busy={installed.isFetching}
            onRetry={() => void installed.refetch()}
          />
        ) : mine.length === 0 ? (
          <Empty
            plain
            title="No package"
            body={
              agent === null
                ? "Save the agent first, then add Pi packages to it from pi.dev."
                : "No Pi package names this agent yet. Add one from pi.dev below."
            }
          />
        ) : (
          <PlainList label="Installed packages">
            {mine.map((each) => (
              <Row
                key={each.name}
                testId="installed-package"
                name={each.name}
                line={each.error ?? each.version}
                kinds={kindsHeld(each.resources)}
                button={
                  <>
                    {waiting.has(each.name) && (
                      <Button
                        size="sm"
                        disabled={actions.busy}
                        aria-label={`Update ${each.name}`}
                        onClick={() =>
                          void actions.run(
                            () => updatePi(each.name),
                            (updates) => reported(tellUpdates(updates), changelogs(updates)),
                            `${each.name} could not be updated. Try again.`,
                          )
                        }
                      >
                        {`Update to ${waiting.get(each.name)?.to}`}
                      </Button>
                    )}
                    <Button
                      variant="secondary"
                      size="sm"
                      disabled={actions.busy}
                      aria-label={`Remove ${each.name}`}
                      onClick={() =>
                        void actions.run(
                          () => removePi(each.name),
                          (offered) => reported([tellOffered(offered)]),
                          `${each.name} could not be removed. Try again.`,
                        )
                      }
                    >
                      Remove
                    </Button>
                  </>
                }
              />
            ))}
          </PlainList>
        )}
      </Box>
      <Paths
        heading="Extension files"
        aside="Files in the repository"
        empty="No extension file."
        placeholder="extensions/log.ts"
        addLabel="Add extension"
        paths={form.extensions}
        onChange={(extensions) => change({ extensions })}
      />
      <Box title="Add from pi.dev" className="flex flex-col gap-3 [&>header]:mb-0">
        <div className="relative">
          <MagnifyingGlassIcon
            aria-hidden="true"
            className="pointer-events-none absolute top-3 left-3 size-4 text-secondary"
          />
          <Input
            aria-label="Search pi.dev"
            placeholder="Search Pi packages"
            value={query}
            className="pl-9"
            onChange={(event) => setQuery(event.target.value)}
          />
        </div>
        <fieldset className="m-0 flex min-w-0 flex-wrap gap-2 border-0 p-0">
          <legend className="sr-only">Filter by kind</legend>
          <Chip selected={kind === null} onClick={() => setKind(null)}>
            All
          </Chip>
          {PI_KINDS.map((each) => (
            <Chip
              key={each.value}
              selected={kind === each.value}
              onClick={() => setKind(each.value)}
            >
              {each.label}
            </Chip>
          ))}
        </fieldset>
        {agent === null && (
          <p className="m-0 text-sm text-secondary">
            A package is named for an agent that exists. Save this agent, then add packages.
          </p>
        )}
        {found.isPending ? (
          <SkeletonList rows={4} message="Searching pi.dev" />
        ) : found.isError ? (
          <ReadFailed
            error={found.error}
            fallback="The catalog could not be searched. Try again."
            busy={found.isFetching}
            onRetry={() => void found.refetch()}
          />
        ) : results.length === 0 ? (
          <Empty
            plain
            title="No package found"
            body={
              settled === ""
                ? "The catalog has nothing of this kind."
                : `Nothing matches "${settled}". Try other words, or all kinds.`
            }
          />
        ) : (
          <PlainList label="Packages">
            {results.map((hit: PiHit) => (
              <Row
                key={hit.name}
                testId="package-result"
                name={hit.name}
                line={[
                  hit.version,
                  hit.weekly_downloads === null ? null : downloads(hit.weekly_downloads),
                  hit.description,
                ]
                  .filter((part) => part !== null && part !== "")
                  .join(" · ")}
                kinds={hit.kinds.slice(0, 1)}
                button={
                  named.has(hit.name) ? (
                    <Tag tone="complete">Added</Tag>
                  ) : (
                    <Button
                      size="sm"
                      variant="secondary"
                      disabled={actions.busy || agent === null}
                      aria-label={`Add ${hit.name}`}
                      title={agent === null ? "Save the agent first" : undefined}
                      onClick={() =>
                        void actions.run(
                          () => proposePi(piSource(hit.name), agent),
                          (offered) => reported([tellOffered(offered)]),
                          `${hit.name} could not be proposed. Try again.`,
                        )
                      }
                    >
                      Add
                    </Button>
                  )
                }
              />
            ))}
          </PlainList>
        )}
      </Box>
    </div>
  );
}
