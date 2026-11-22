/**
 * The Installed tab of the sheet: what is installed from the registries of Moebius, each with its version and an
 * Update when the nightly check found a newer one, and Remove.
 */
import { useQuery } from "@tanstack/react-query";
import { installedPackages, removePackage, updatePackage } from "../api/calls/packages";
import { keys } from "../api/keys";
import { Tag } from "../ui/chat/Tag";
import { Button } from "../ui/components/Button";
import { Empty } from "../ui/page/Empty";
import { ReadFailed } from "../ui/page/ReadFailed";
import { SkeletonList } from "../ui/page/Skeleton";
import { installedStanding, installsAtOnce, kindLabel, standingTags, tellOutcome } from "./model";
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
  actions,
  onOpen,
}: {
  actions: Actions;
  /** Opens a package. */
  onOpen: (name: string) => void;
}) {
  const reading = useQuery({ queryKey: keys.packagesInstalled, queryFn: installedPackages });
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
  if (reading.data.length === 0) {
    return (
      <Empty
        plain
        title="Nothing installed"
        body="A plugin, workflow, rules or skills package you install from the registries is listed here."
      />
    );
  }
  return (
    <PlainList label="Installed packages" className="gap-2">
      {reading.data.map((each) => {
        const state = installedStanding(each);
        return (
          <Row
            key={each.name}
            name={each.name}
            tags={[
              { label: kindLabel(each.kind), tone: "plain" },
              ...(each.enabled === false ? [{ label: "Off", tone: "plain" as const }] : []),
              ...standingTags(state),
            ]}
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
      })}
    </PlainList>
  );
}
