/**
 * The Installed tab of the sheet: what is installed from the catalog the sheet is pointed at, each with its version and
 * an Update when one is waiting (the nightly check's newer version for a Moebius package; the Inbox item of the
 * nightly check for a Pi package), and Remove. The Pi packages also have Update all, which proposes every newer version
 * at once.
 */
import { useQuery } from "@tanstack/react-query";
import {
  installedPackages,
  installedPi,
  removePackage,
  removePi,
  updateAllPi,
  updatePackage,
  updatePi,
} from "../api/calls/packages";
import { keys } from "../api/keys";
import { Tag } from "../ui/chat/Tag";
import { Button } from "../ui/components/Button";
import { Empty } from "../ui/page/Empty";
import { ReadFailed } from "../ui/page/ReadFailed";
import { SkeletonList } from "../ui/page/Skeleton";
import {
  type Catalog,
  changelogs,
  installedStanding,
  installsAtOnce,
  kindLabel,
  standing,
  standingTags,
  tellOffered,
  tellOutcome,
  tellUpdates,
} from "./model";
import { PlainList, RemoveButton } from "./parts";
import { reported } from "./Report";
import type { Actions } from "./useActions";

function Row({
  name,
  tags,
  line,
  buttons,
  onOpen,
}: {
  name: string;
  tags: Array<{ label: string; tone: "plain" | "attention" | "complete" | "error" }>;
  line: string;
  buttons: React.ReactNode;
  onOpen: () => void;
}) {
  return (
    <li
      data-testid="installed-package"
      className="flex flex-col gap-2 rounded-control bg-sunken p-3"
    >
      <div className="flex min-w-0 flex-wrap items-center gap-2">
        <h3 className="m-0 min-w-0 break-all font-mono text-sm font-semibold text-text">{name}</h3>
        {tags.map((tag) => (
          <Tag key={tag.label} tone={tag.tone}>
            {tag.label}
          </Tag>
        ))}
      </div>
      {line && <p className="m-0 min-w-0 break-words text-sm text-secondary">{line}</p>}
      <div className="flex flex-wrap items-center gap-2">
        <Button variant="ghost" size="sm" aria-label={`Details of ${name}`} onClick={onOpen}>
          Details
        </Button>
        {buttons}
      </div>
    </li>
  );
}

export function Installed({
  catalog,
  actions,
  waiting,
  onOpen,
}: {
  catalog: Catalog;
  actions: Actions;
  /** The Pi packages with an update waiting in the Inbox. */
  waiting: Map<string, { from: string; to: string }>;
  /** Opens a package; an installed Pi package carries its source, which is not always npm. */
  onOpen: (name: string, source?: string) => void;
}) {
  const moebius = useQuery({
    queryKey: keys.packagesInstalled,
    queryFn: installedPackages,
    enabled: catalog === "moebius",
  });
  const pi = useQuery({
    queryKey: keys.piInstalled,
    queryFn: installedPi,
    enabled: catalog === "pi",
  });
  const reading = catalog === "moebius" ? moebius : pi;
  if (reading.isPending) return <SkeletonList rows={3} message="Reading what is installed" />;
  if (reading.isError) {
    return (
      <ReadFailed
        error={reading.error}
        fallback="What is installed could not be read. Try again."
        busy={reading.isFetching}
        onRetry={() => void reading.refetch()}
      />
    );
  }
  const empty = catalog === "moebius" ? moebius.data?.length === 0 : pi.data?.length === 0;
  if (empty) {
    return (
      <Empty
        plain
        title="Nothing installed"
        body={
          catalog === "moebius"
            ? "A plugin, workflow, rules or skills package you install from the registries is listed here."
            : "A Pi package you install is listed here once a person has merged the change."
        }
      />
    );
  }
  return (
    <div className="flex flex-col gap-3">
      {catalog === "pi" && (
        <div>
          <Button
            size="sm"
            variant="secondary"
            disabled={actions.busy}
            onClick={() =>
              void actions.run(
                updateAllPi,
                (updates) => reported(tellUpdates(updates), changelogs(updates)),
                "The packages could not be updated. Try again.",
              )
            }
          >
            Update all
          </Button>
        </div>
      )}
      <PlainList label="Installed packages" className="gap-2">
        {catalog === "moebius"
          ? (moebius.data ?? []).map((each) => {
              const state = installedStanding(each);
              return (
                <Row
                  key={each.name}
                  name={each.name}
                  tags={[{ label: kindLabel(each.kind), tone: "plain" }, ...standingTags(state)]}
                  line={`From ${each.registry}${each.repository ? `, proposed to ${each.repository}` : ""}`}
                  onOpen={() => onOpen(each.name)}
                  buttons={
                    <>
                      {each.update_available !== null && (
                        <Button
                          size="sm"
                          disabled={actions.busy}
                          aria-label={`Update ${each.name}`}
                          onClick={() =>
                            void actions.run(
                              () => updatePackage(each.name),
                              (outcome) => reported([tellOutcome(outcome)]),
                              `${each.name} could not be updated. Try again.`,
                            )
                          }
                        >
                          Update
                        </Button>
                      )}
                      <RemoveButton
                        name={each.name}
                        busy={actions.busy}
                        confirm={installsAtOnce(each.kind)}
                        onRemove={() =>
                          void actions.run(
                            () => removePackage(each.name),
                            (outcome) => reported([tellOutcome(outcome)]),
                            `${each.name} could not be removed. Try again.`,
                          )
                        }
                      />
                    </>
                  }
                />
              );
            })
          : (pi.data ?? []).map((each) => {
              const state = standing(each.version, waiting.get(each.name)?.to ?? null);
              return (
                <Row
                  key={each.name}
                  name={each.name}
                  tags={[
                    ...standingTags(state),
                    ...(each.error ? [{ label: "Files unreadable", tone: "error" as const }] : []),
                  ]}
                  line={
                    each.error ??
                    (each.agents.length === 0
                      ? "No agent names it."
                      : `Named by ${each.agents.join(", ")}.`)
                  }
                  onOpen={() => onOpen(each.name, each.source)}
                  buttons={
                    <>
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
                        Update
                      </Button>
                      <RemoveButton
                        name={each.name}
                        busy={actions.busy}
                        confirm={false}
                        onRemove={() =>
                          void actions.run(
                            () => removePi(each.name),
                            (offered) => reported([tellOffered(offered)]),
                            `${each.name} could not be removed. Try again.`,
                          )
                        }
                      />
                    </>
                  }
                />
              );
            })}
      </PlainList>
    </div>
  );
}
