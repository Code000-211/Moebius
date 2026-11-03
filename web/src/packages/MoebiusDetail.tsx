/**
 * A package of the Moebius registries opened in the sheet: what it is, its versions with whether each works with this
 * Moebius, where its files go, and what can be done with it. A plugin is installed at once; a workflow, rules or
 * skills package is proposed as a change a person merges, and the sheet says so before the press, with a choice of
 * the repository when there are several. An installed package is updated (when a newer version works) or removed.
 */
import { useQuery } from "@tanstack/react-query";
import { useState } from "react";
import { installPackage, packageInfo, removePackage, updatePackage } from "../api/calls/packages";
import { keys } from "../api/keys";
import { Tag } from "../ui/chat/Tag";
import { Button } from "../ui/components/Button";
import { Select } from "../ui/page/Field";
import { ReadFailed } from "../ui/page/ReadFailed";
import { SkeletonLines } from "../ui/page/Skeleton";
import { installsAtOnce, kindLabel, needsRepository, tellOutcome } from "./model";
import { Back, Fact, Lines, Outside, PlainList, RemoveButton } from "./parts";
import { reported } from "./Report";
import type { Actions } from "./useActions";

export function MoebiusDetail({
  name,
  actions,
  repositories,
  back,
  onBack,
}: {
  name: string;
  actions: Actions;
  /** The repositories a workflow or rules package can be proposed to, by name. */
  repositories: string[];
  back: string;
  onBack: () => void;
}) {
  const info = useQuery({ queryKey: keys.packageInfo(name), queryFn: () => packageInfo(name) });
  const [chosen, setChosen] = useState<string | null>(null);

  if (info.isPending) {
    return (
      <>
        <Back label={back} onClick={onBack} />
        <SkeletonLines lines={5} message={`Reading ${name}`} />
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
  const { installed, registry, versions } = info.data;
  const newest = versions.find((each) => each.compatible) ?? versions[0];
  const kind = newest?.kind ?? installed?.kind ?? "plugin";
  const update =
    installed !== null && newest?.compatible === true && newest.version !== installed.version
      ? newest.version
      : null;
  const repository = chosen ?? repositories[0] ?? null;
  const request = {
    name,
    registry,
    ...(needsRepository(kind) && repositories.length > 1 && repository !== null
      ? { repository }
      : {}),
  };

  return (
    <div className="flex flex-col gap-4" data-testid="package-detail">
      <Back label={back} onClick={onBack} />
      <div className="flex flex-wrap items-center gap-2">
        <h3 className="m-0 min-w-0 break-all font-mono text-base font-semibold text-text">
          {name}
        </h3>
        <Tag>{kindLabel(kind)}</Tag>
        {installed && <Tag tone="complete">{`Installed ${installed.version}`}</Tag>}
        {update && <Tag tone="attention">{`Update to ${update}`}</Tag>}
      </div>
      {newest?.description && <p className="m-0 text-base text-text">{newest.description}</p>}
      <div className="grid gap-4 sm:grid-cols-2">
        <Fact label="Registry">{registry}</Fact>
        {newest && <Fact label="Newest">{newest.version}</Fact>}
        {newest?.license && <Fact label="License">{newest.license}</Fact>}
        {newest?.homepage && (
          <Fact label="Homepage">
            <Outside href={newest.homepage}>{newest.homepage}</Outside>
          </Fact>
        )}
        {newest?.plugin && (
          <Fact label="Runs as">{`${newest.plugin.kind} (${newest.plugin.program})`}</Fact>
        )}
      </div>
      {newest?.plugin?.decides && newest.plugin.decides.length > 0 && (
        <Lines label="Answers" lines={newest.plugin.decides} />
      )}
      <Lines label="Files" lines={(newest?.files ?? []).map((file) => file.to)} />

      {installed ? (
        <div className="flex flex-wrap items-center gap-2">
          {update && (
            <Button
              size="sm"
              disabled={actions.busy}
              onClick={() =>
                void actions.run(
                  () => updatePackage(name),
                  (outcome) => reported([tellOutcome(outcome)]),
                  `${name} could not be updated. Try again.`,
                )
              }
            >
              {`Update to ${update}`}
            </Button>
          )}
          <RemoveButton
            name={name}
            busy={actions.busy}
            confirm={installsAtOnce(kind)}
            onRemove={() =>
              void actions.run(
                () => removePackage(name),
                (outcome) => reported([tellOutcome(outcome)]),
                `${name} could not be removed. Try again.`,
              )
            }
          />
        </div>
      ) : (
        <div className="flex flex-col gap-3">
          <p className="m-0 text-sm text-secondary">
            {installsAtOnce(kind)
              ? "It is installed at once, and starts with the server."
              : "Its files are proposed as a change. Nothing is written until a person merges it."}
          </p>
          {needsRepository(kind) && repositories.length > 1 && (
            <div className="max-w-60">
              <Select
                aria-label="Repository"
                options={repositories}
                value={repository ?? ""}
                onChange={setChosen}
              />
            </div>
          )}
          <div>
            <Button
              disabled={actions.busy}
              onClick={() =>
                void actions.run(
                  () => installPackage(request),
                  (outcome) => reported([tellOutcome(outcome)]),
                  `${name} could not be installed. Try again.`,
                )
              }
            >
              {installsAtOnce(kind) ? "Install" : "Propose install"}
            </Button>
          </div>
        </div>
      )}

      <div className="flex flex-col gap-2">
        <span className="text-sm font-semibold text-secondary">Versions</span>
        <PlainList label="Versions" className="gap-1">
          {versions.map((each) => (
            <li key={each.version} className="flex flex-wrap items-center justify-between gap-2">
              <span className="text-base text-text tabular-nums">{each.version}</span>
              <span className="text-sm text-secondary">
                {each.compatible ? "Works with this Moebius" : `Needs Moebius ${each.moebius}`}
              </span>
            </li>
          ))}
        </PlainList>
      </div>
    </div>
  );
}
