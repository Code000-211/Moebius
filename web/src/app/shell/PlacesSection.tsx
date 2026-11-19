/**
 * The person's pins and the places they opened lately, as two lists of the wide sidebar: Pinned, each row with the
 * buttons that move it up or down and remove it, and Recent, each row with the button that pins it. Both are kept on
 * the server for the person (`/v1/places`), so they are the same in every browser they sign in on. The rail has no
 * room for either and shows neither. A list with nothing in it is not shown.
 */
import {
  ArrowDownIcon,
  ArrowUpIcon,
  FilesIcon,
  FolderIcon,
  type Icon,
  ListBulletsIcon,
  PushPinIcon,
  SquaresFourIcon,
  XIcon,
} from "@phosphor-icons/react";
import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { Link } from "react-router";
import { forgetRecent, pin, places, reorderPins, unpin } from "../../api/calls/places";
import { keys } from "../../api/keys";
import type { Place, PlaceKind, Places } from "../../api/types/places";
import { IconButton } from "../../ui/components/IconButton";
import { cn } from "../../ui/lib/cn";
import { moved, placePath } from "./places";

const ICONS: Record<PlaceKind, Icon> = {
  project: FolderIcon,
  card: SquaresFourIcon,
  view: ListBulletsIcon,
  page: FilesIcon,
};

/** One row: the link to the place and, beside it, buttons that show while the row is hovered or has focus. */
function PlaceRow({
  place,
  current,
  children,
}: {
  place: Place;
  current: boolean;
  children: React.ReactNode;
}) {
  const Mark = ICONS[place.kind];
  return (
    <li className="group flex items-center rounded-control hover:bg-hover">
      <Link
        to={placePath(place)}
        aria-current={current ? "page" : undefined}
        title={place.title}
        className={cn(
          "flex min-h-8 min-w-0 flex-1 items-center gap-2 rounded-control px-3 text-base",
          current ? "font-semibold text-text" : "text-sidebar-text hover:text-text",
        )}
      >
        <Mark className="size-5 shrink-0" weight="regular" />
        <span className="min-w-0 flex-1 truncate">{place.title}</span>
      </Link>
      <span className="flex shrink-0 opacity-0 transition-opacity group-focus-within:opacity-100 group-hover:opacity-100">
        {children}
      </span>
    </li>
  );
}

export function PlacesSection({ at }: { at: string }) {
  const client = useQueryClient();
  const kept = useQuery({ queryKey: keys.places, queryFn: places });
  const change = useMutation({
    mutationFn: (call: () => Promise<Places>) => call(),
    onSuccess: (next) => client.setQueryData(keys.places, next),
  });
  const favourites = kept.data?.favourites ?? [];
  // A place already pinned is not offered again from the recent list.
  const recent = (kept.data?.recent ?? []).filter(
    (place) =>
      !favourites.some(
        (fav) =>
          fav.kind === place.kind &&
          fav.repository_id === place.repository_id &&
          fav.key === place.key,
      ),
  );
  const here = (place: Place) => placePath(place).split("?")[0] === at && place.kind === "project";
  return (
    <>
      {favourites.length > 0 && (
        <Group title="Pinned">
          {favourites.map((place, index) => (
            <PlaceRow key={place.id} place={place} current={here(place)}>
              <IconButton
                icon={ArrowUpIcon}
                aria-label={`Move ${place.title} up`}
                disabled={index === 0}
                onClick={() => change.mutate(() => reorderPins(moved(favourites, index, -1)))}
              />
              <IconButton
                icon={ArrowDownIcon}
                aria-label={`Move ${place.title} down`}
                disabled={index === favourites.length - 1}
                onClick={() => change.mutate(() => reorderPins(moved(favourites, index, 1)))}
              />
              <IconButton
                icon={XIcon}
                aria-label={`Unpin ${place.title}`}
                onClick={() => change.mutate(() => unpin(place.id))}
              />
            </PlaceRow>
          ))}
        </Group>
      )}
      {recent.length > 0 && (
        <Group title="Recent">
          {recent.map((place) => (
            <PlaceRow key={place.id} place={place} current={here(place)}>
              <IconButton
                icon={PushPinIcon}
                aria-label={`Pin ${place.title}`}
                onClick={() =>
                  change.mutate(() =>
                    pin({
                      kind: place.kind,
                      repository_id: place.repository_id,
                      key: place.key,
                      title: place.title,
                    }),
                  )
                }
              />
              <IconButton
                icon={XIcon}
                aria-label={`Forget ${place.title}`}
                onClick={() => change.mutate(() => forgetRecent(place.id))}
              />
            </PlaceRow>
          ))}
        </Group>
      )}
    </>
  );
}

function Group({ title, children }: { title: string; children: React.ReactNode }) {
  return (
    <div className="py-2">
      <div className="mb-2 flex h-4 items-center px-2 text-sm leading-4 font-semibold tracking-wide text-sidebar-group [font-variant-caps:all-small-caps]">
        {title}
      </div>
      {/* biome-ignore lint/a11y/noRedundantRoles: preflight sets list-style: none, and WebKit then drops a list with
          no marker from the accessibility tree entirely */}
      <ul role="list" className="flex flex-col gap-1">
        {children}
      </ul>
    </div>
  );
}
