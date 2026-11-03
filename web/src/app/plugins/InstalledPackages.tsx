/**
 * The packages installed from the registries, on the Plugins page: each with its kind, its version and, when the
 * nightly check found a newer one that works with this Moebius, an Update. What an update told (or why it was refused)
 * is said under the heading. Reading, searching and removing are the package sheet's.
 */
import { useQuery } from "@tanstack/react-query";
import { useState } from "react";
import { installedPackages, updatePackage } from "../../api/calls/packages";
import { keys } from "../../api/keys";
import { installedStanding, kindLabel, tellOutcome } from "../../packages/model";
import { PlainList } from "../../packages/parts";
import { type Report, ReportView, reported } from "../../packages/Report";
import { useActions } from "../../packages/useActions";
import { Tag } from "../../ui/chat/Tag";
import { Button } from "../../ui/components/Button";
import { Box } from "../../ui/page/Box";
import { Empty } from "../../ui/page/Empty";
import { ReadFailed } from "../../ui/page/ReadFailed";
import { SkeletonCard } from "../../ui/page/Skeleton";

export function InstalledPackages({ onBrowse }: { onBrowse: () => void }) {
  const installed = useQuery({ queryKey: keys.packagesInstalled, queryFn: installedPackages });
  const [report, setReport] = useState<Report | null>(null);
  const actions = useActions(setReport);

  if (installed.isPending)
    return <SkeletonCard lines={2} message="Reading the installed packages" />;
  if (installed.isError) {
    return (
      <ReadFailed
        error={installed.error}
        fallback="The installed packages could not be read. Try again."
        busy={installed.isFetching}
        onRetry={() => void installed.refetch()}
      />
    );
  }
  return (
    <Box title="Installed packages" className="flex flex-col gap-4" testId="installed-packages">
      {report && <ReportView report={report} onLeave={() => {}} />}
      {installed.data.length === 0 ? (
        <Empty
          plain
          title="No package installed"
          body="Plugins, workflows, rules and skills from the registries are installed from the package sheet."
          action={
            <Button size="sm" variant="secondary" onClick={onBrowse}>
              Browse packages
            </Button>
          }
        />
      ) : (
        <PlainList label="Installed packages" className="gap-4">
          {installed.data.map((each) => {
            const state = installedStanding(each);
            return (
              <li
                key={each.name}
                data-testid="installed-package"
                className="flex flex-wrap items-center justify-between gap-3"
              >
                <div className="flex min-w-0 flex-wrap items-center gap-2">
                  <h3 className="m-0 min-w-0 break-all font-mono text-sm font-semibold text-text">
                    {each.name}
                  </h3>
                  <Tag>{kindLabel(each.kind)}</Tag>
                  {state.map((each) => (
                    <Tag
                      key={each.label}
                      tone={each.tone === "attention" ? "attention" : "complete"}
                    >
                      {each.label}
                    </Tag>
                  ))}
                </div>
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
                    {`Update to ${each.update_available}`}
                  </Button>
                )}
              </li>
            );
          })}
        </PlainList>
      )}
    </Box>
  );
}
