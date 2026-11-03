/**
 * A package of the pi.dev catalog opened in the sheet: what it holds (extensions, skills, prompts, themes), what npm
 * would run that Moebius never does, and what can be done with it. Every action is a proposal, a change of the harness
 * configuration repository that a person merges, so the sheet says so before the press and links to the change after
 * it. From an agent's Extensions the proposal names that agent, which is given the package's extensions and skills.
 */
import { useQuery } from "@tanstack/react-query";
import { useEffect, useState } from "react";
import { piInfo, proposePi, removePi, updatePi } from "../api/calls/packages";
import { keys } from "../api/keys";
import type { PiHit, PiResources } from "../api/types/packages";
import { Tag } from "../ui/chat/Tag";
import { Button } from "../ui/components/Button";
import { plural } from "../ui/lib/format";
import { ReadFailed } from "../ui/page/ReadFailed";
import { SkeletonLines } from "../ui/page/Skeleton";
import { changelogs, downloads, kindLabel, tellOffered, tellUpdates } from "./model";
import { Back, Fact, Lines, Outside, RemoveButton } from "./parts";
import { reported } from "./Report";
import type { Actions } from "./useActions";

/** What a package holds, one line to a kind that it has any of. */
export function holdings(resources: PiResources): string[] {
  const kinds: Array<[string, string[]]> = [
    ["extension", resources.extensions],
    ["skill", resources.skills],
    ["prompt", resources.prompts],
    ["theme", resources.themes],
  ];
  return kinds
    .filter(([, paths]) => paths.length > 0)
    .map(([kind, paths]) => `${plural(paths.length, kind, `${kind}s`)}: ${paths.join(", ")}`);
}

export function PiDetail({
  name,
  hit,
  source,
  agent,
  actions,
  back,
  onBack,
  waiting,
}: {
  name: string;
  /** Where it is fetched from: what the installed package was added from, or npm for one found by search. */
  source: string;
  /** What the search knew of it: kinds, downloads and links. */
  hit?: PiHit;
  /** The agent a proposal names the package for. */
  agent: string | null;
  actions: Actions;
  back: string;
  onBack: () => void;
  /** The version an update waiting in the Inbox goes to. */
  waiting: string | null;
}) {
  // A proposal that was made waits for a person to merge it: pressing Propose again would be a second change, so the
  // button rests until the package's state moves (the change merged, or the package opened again).
  const [proposed, setProposed] = useState(false);
  const info = useQuery({
    queryKey: keys.piInfo(source),
    queryFn: () => piInfo(source),
    // Fetching a package from npm takes a while and finds the same files for the same version.
    staleTime: 60_000,
  });
  const installedNow = info.data?.installed ?? null;
  // biome-ignore lint/correctness/useExhaustiveDependencies: it runs when the installed version changes
  useEffect(() => setProposed(false), [installedNow, source]);
  if (info.isPending) {
    return (
      <>
        <Back label={back} onClick={onBack} />
        <SkeletonLines lines={5} message={`Fetching ${name}`} />
      </>
    );
  }
  if (info.isError) {
    return (
      <>
        <Back label={back} onClick={onBack} />
        <ReadFailed
          error={info.error}
          fallback={`${name} could not be read. Try again.`}
          busy={info.isFetching}
          onRetry={() => void info.refetch()}
        />
      </>
    );
  }
  const data = info.data;
  return (
    <div className="flex flex-col gap-4" data-testid="package-detail">
      <Back label={back} onClick={onBack} />
      <div className="flex flex-wrap items-center gap-2">
        <h3 className="m-0 min-w-0 break-all font-mono text-base font-semibold text-text">
          {name}
        </h3>
        {(hit?.kinds ?? []).map((kind) => (
          <Tag key={kind}>{kindLabel(kind)}</Tag>
        ))}
        {data.installed !== null && <Tag tone="complete">{`Installed ${data.installed}`}</Tag>}
        {waiting !== null && <Tag tone="attention">{`Update to ${waiting}`}</Tag>}
      </div>
      {data.description && <p className="m-0 text-base text-text">{data.description}</p>}
      <div className="grid gap-4 sm:grid-cols-2">
        <Fact label="Version">{data.version}</Fact>
        <Fact label="Source">
          <span className="font-mono text-sm">{data.source}</span>
        </Fact>
        {hit?.weekly_downloads != null && (
          <Fact label="Downloads">{downloads(hit.weekly_downloads)}</Fact>
        )}
        <Fact label="Files">{String(data.files)}</Fact>
        {hit?.npm && (
          <Fact label="npm">
            <Outside href={hit.npm}>{hit.npm}</Outside>
          </Fact>
        )}
        {hit?.homepage && (
          <Fact label="Homepage">
            <Outside href={hit.homepage}>{hit.homepage}</Outside>
          </Fact>
        )}
      </div>
      <Lines label="What it holds" lines={holdings(data.resources)} />
      <Lines label="Dependencies it adds" lines={data.dependencies} />
      {data.install_scripts.length > 0 && (
        <p className="m-0 text-sm text-secondary">
          {`npm would run ${data.install_scripts.join(", ")} on an install. Moebius never does.`}
        </p>
      )}

      {data.installed !== null ? (
        <div className="flex flex-wrap items-center gap-2">
          <Button
            size="sm"
            disabled={actions.busy}
            onClick={() =>
              void actions.run(
                () => updatePi(name),
                (updates) => reported(tellUpdates(updates), changelogs(updates)),
                `${name} could not be updated. Try again.`,
              )
            }
          >
            Update
          </Button>
          <RemoveButton
            name={name}
            busy={actions.busy}
            confirm={false}
            onRemove={() =>
              void actions.run(
                () => removePi(name),
                (offered) => reported([tellOffered(offered)]),
                `${name} could not be removed. Try again.`,
              )
            }
          />
        </div>
      ) : (
        <div className="flex flex-col gap-3">
          <p className="m-0 text-sm text-secondary">
            {agent === null
              ? "Installing is a change of the harness configuration repository. Nothing is installed until a person merges it."
              : `Installing is a change of the harness configuration repository, and names the package for ${agent}. Nothing is installed until a person merges it.`}
          </p>
          <div>
            <Button
              disabled={actions.busy || proposed}
              onClick={() =>
                void actions.run(
                  () => proposePi(source, agent),
                  (offered) => {
                    setProposed(true);
                    return reported([tellOffered(offered)]);
                  },
                  `${name} could not be proposed. Try again.`,
                )
              }
            >
              {agent === null ? "Propose install" : `Propose install for ${agent}`}
            </Button>
          </div>
        </div>
      )}
    </div>
  );
}
