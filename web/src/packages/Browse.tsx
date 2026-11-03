/**
 * The Browse tab of the sheet: a search field over the catalog (the registries of Moebius, or the pi.dev catalog), a
 * filter by kind, and the results, each with its name, kind, version, description and whether it is installed or has
 * an update. The search runs when the typing pauses. A registry that cannot be read is said above the results of the
 * others; choosing a result opens the package.
 */
import { MagnifyingGlassIcon } from "@phosphor-icons/react";
import { keepPreviousData, useQuery } from "@tanstack/react-query";
import type { RefObject } from "react";
import { searchPackages, searchPi } from "../api/calls/packages";
import { keys } from "../api/keys";
import type { PackageSummary, PiHit } from "../api/types/packages";
import { Input } from "../ui/components/Input";
import { useSettled } from "../ui/lib/useSettled";
import { Chip } from "../ui/page/Chip";
import { Empty } from "../ui/page/Empty";
import { ListCard } from "../ui/page/List";
import { Notice } from "../ui/page/Notice";
import { ReadFailed } from "../ui/page/ReadFailed";
import { SkeletonList } from "../ui/page/Skeleton";
import {
  type Catalog,
  downloads,
  inKind,
  kindLabel,
  kindsOf,
  piInKind,
  problemLine,
  type Standing,
  standing,
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
  catalog,
  browsing,
  onBrowsing,
  onOpen,
  installed,
  waiting,
  searchRef,
}: {
  catalog: Catalog;
  browsing: Browsing;
  onBrowsing: (browsing: Browsing) => void;
  /** Opens a package; a Pi package carries what the search knew of it. */
  onOpen: (name: string, hit?: PiHit) => void;
  /** What is installed, to mark the results of the Pi catalog (the Moebius search says it itself). */
  installed: Map<string, string>;
  /** The Pi packages that have an update waiting: the version it goes to. */
  waiting: Map<string, { from: string; to: string }>;
  /** The search field, which the sheet puts the focus on when it opens. */
  searchRef: RefObject<HTMLInputElement | null>;
}) {
  const settled = useSettled(browsing.query).trim();
  const moebius = useQuery({
    queryKey: keys.packageSearch(settled),
    queryFn: () => searchPackages(settled),
    enabled: catalog === "moebius",
    placeholderData: keepPreviousData,
  });
  const pi = useQuery({
    queryKey: keys.piSearch(settled, browsing.kind),
    queryFn: () => searchPi(settled, browsing.kind),
    enabled: catalog === "pi",
    placeholderData: keepPreviousData,
  });
  const reading = catalog === "moebius" ? moebius : pi;
  const found: Array<{
    key: string;
    name: string;
    kind: string;
    state: Standing[];
    line: string;
    hit?: PiHit;
  }> =
    catalog === "moebius"
      ? inKind(moebius.data?.packages ?? [], browsing.kind).map((each: PackageSummary) => ({
          key: `${each.registry}/${each.name}`,
          name: each.name,
          kind: each.kind,
          state: summaryStanding(each),
          line: `${each.version}${each.compatible ? "" : " (does not work with this Moebius)"} · ${each.description}`,
        }))
      : piInKind(pi.data ?? [], browsing.kind).map((each) => ({
          key: each.name,
          name: each.name,
          kind: each.kinds[0] ?? "extension",
          state: standing(installed.get(each.name) ?? null, waiting.get(each.name)?.to ?? null),
          line: [
            each.version,
            each.weekly_downloads === null ? null : downloads(each.weekly_downloads),
            each.description,
          ]
            .filter((part) => part !== null && part !== "")
            .join(" · "),
          hit: each,
        }));

  return (
    <div className="flex flex-col gap-3">
      <div className="relative">
        <MagnifyingGlassIcon
          aria-hidden="true"
          className="pointer-events-none absolute top-3 left-3 size-4 text-secondary"
        />
        <Input
          aria-label={catalog === "moebius" ? "Search the registries" : "Search pi.dev"}
          placeholder={catalog === "moebius" ? "Search packages" : "Search Pi packages"}
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
        {kindsOf(catalog).map((each) => (
          <Chip
            key={each.value}
            selected={browsing.kind === each.value}
            onClick={() => onBrowsing({ ...browsing, kind: each.value })}
          >
            {each.label}
          </Chip>
        ))}
      </fieldset>
      {catalog === "moebius" &&
        moebius.data?.problems.map((problem) => (
          <Notice key={problem.registry} tone="error">
            {problemLine(problem.registry, problem.message)}
          </Notice>
        ))}
      {reading.isPending ? (
        <SkeletonList
          rows={4}
          message={catalog === "moebius" ? "Searching the registries" : "Searching pi.dev"}
        />
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
                onClick={() => onOpen(each.name, each.hit)}
              />
            </li>
          ))}
        </PlainList>
      )}
    </div>
  );
}
