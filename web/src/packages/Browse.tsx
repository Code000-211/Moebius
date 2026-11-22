/**
 * The Browse tab of the sheet: a search field over the registries of Moebius, a
 * filter by kind, and the results, each with its name, kind, version, description and whether it is installed or has
 * an update. The search runs when the typing pauses. A registry that cannot be read is said above the results of the
 * others; choosing a result opens the package.
 */
import { MagnifyingGlassIcon } from "@phosphor-icons/react";
import { keepPreviousData, useQuery } from "@tanstack/react-query";
import type { RefObject } from "react";
import { searchPackages } from "../api/calls/packages";
import { keys } from "../api/keys";
import type { PackageSummary } from "../api/types/packages";
import { Input } from "../ui/components/Input";
import { useSettled } from "../ui/lib/useSettled";
import { Chip } from "../ui/page/Chip";
import { Empty } from "../ui/page/Empty";
import { ListCard } from "../ui/page/List";
import { Notice } from "../ui/page/Notice";
import { ReadFailed } from "../ui/page/ReadFailed";
import { SkeletonList } from "../ui/page/Skeleton";
import {
  inKind,
  kindLabel,
  MOEBIUS_KINDS,
  problemLine,
  type Standing,
  standingTags,
  summaryStanding,
} from "./model";
import { PlainList } from "./parts";

export interface Browsing {
  query: string;
  kind: string | null;
}

/** A result's tags: what kind it is, and whether it is installed or has an update waiting. */
const tags = (kind: string, state: Standing[]) => [
  { label: kindLabel(kind), tone: "plain" as const },
  ...standingTags(state),
];

export function Browse({
  browsing,
  onBrowsing,
  onOpen,
  searchRef,
}: {
  browsing: Browsing;
  onBrowsing: (browsing: Browsing) => void;
  /** Opens a package. */
  onOpen: (name: string) => void;
  /** The search field, which the sheet puts the focus on when it opens. */
  searchRef: RefObject<HTMLInputElement | null>;
}) {
  const settled = useSettled(browsing.query).trim();
  const reading = useQuery({
    queryKey: keys.packageSearch(settled),
    queryFn: () => searchPackages(settled),
    placeholderData: keepPreviousData,
  });
  const found = inKind(reading.data?.packages ?? [], browsing.kind).map((each: PackageSummary) => ({
    key: `${each.registry}/${each.name}`,
    name: each.name,
    kind: each.kind,
    state: summaryStanding(each),
    line: `${each.version}${each.compatible ? "" : " (does not work with this Moebius)"} · ${each.description}`,
  }));

  return (
    <div className="flex flex-col gap-3">
      <div className="relative">
        <MagnifyingGlassIcon
          aria-hidden="true"
          className="pointer-events-none absolute top-3 left-3 size-4 text-secondary"
        />
        <Input
          aria-label="Search the registries"
          placeholder="Search packages"
          ref={searchRef}
          value={browsing.query}
          className="pl-9"
          onChange={(event) => onBrowsing({ ...browsing, query: event.target.value })}
        />
      </div>
      <fieldset className="m-0 flex min-w-0 flex-wrap gap-2 border-0 p-0">
        <legend className="sr-only">Filter by kind</legend>
        <Chip
          selected={browsing.kind === null}
          onClick={() => onBrowsing({ ...browsing, kind: null })}
        >
          All
        </Chip>
        {MOEBIUS_KINDS.map((each) => (
          <Chip
            key={each.value}
            selected={browsing.kind === each.value}
            onClick={() => onBrowsing({ ...browsing, kind: each.value })}
          >
            {each.label}
          </Chip>
        ))}
      </fieldset>
      {reading.data?.problems.map((problem) => (
        <Notice key={problem.registry} tone="error">
          {problemLine(problem.registry, problem.message)}
        </Notice>
      ))}
      {reading.isPending ? (
        <SkeletonList rows={4} message="Searching the registries" />
      ) : reading.isError ? (
        <ReadFailed
          error={reading.error}
          fallback="The catalog could not be searched. Try again."
          busy={reading.isFetching}
          onRetry={() => void reading.refetch()}
        />
      ) : found.length === 0 ? (
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
        <PlainList label="Packages" className="gap-2">
          {found.map((each) => (
            <li key={each.key}>
              <ListCard
                title={each.name}
                tags={tags(each.kind, each.state)}
                line={each.line}
                selected={false}
                onClick={() => onOpen(each.name)}
              />
            </li>
          ))}
        </PlainList>
      )}
    </div>
  );
}
