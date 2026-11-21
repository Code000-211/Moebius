/** Where a kept place leads, which place an address is, and the order a pin moves to. */
import type { Place, PlaceRef } from "../../api/types/places";
import { boardPath, cardPath } from "../pages";

/**
 * The address a place leads to, or null when `homeOf` does not know the project the place's repository is at. Views and
 * pages have no page of their own yet, so they lead to their project's board.
 */
export function placePath(
  place: Place,
  homeOf: (repository: number) => number | undefined,
): string | null {
  const project = homeOf(place.repository_id);
  if (project === undefined) return null;
  if (place.kind === "card") return cardPath(project, Number(place.key), place.repository_id);
  return boardPath(project, place.repository_id);
}

/**
 * The place an address is, if it is one: a project's board or page, or a card open on a board. The board a project
 * address shows is the repository `?repository=` names, else the one `firstOf` gives for the project.
 */
export function placeAt(
  pathname: string,
  search: string,
  firstOf: (project: number) => number | undefined,
): PlaceRef | null {
  const match = /^\/projects\/(\d+)(?:\/|$)/.exec(pathname);
  if (!match) return null;
  const params = new URLSearchParams(search);
  const card = params.get("card");
  if (card !== null && /^\d+$/.test(card)) return { kind: "card", key: card };
  const repository = Number(params.get("repository")) || firstOf(Number(match[1]));
  return repository === undefined ? null : { kind: "project", repository_id: repository };
}

/** Whether `place` is `ref`: the same kind, repository and key. A card is named by its key alone. */
export function samePlace(place: Place, ref: PlaceRef): boolean {
  if (place.kind !== ref.kind) return false;
  if (ref.kind === "card") return place.key === ref.key;
  return place.repository_id === ref.repository_id && place.key === (ref.key ?? "");
}

/** The ids of `pins` with the pin at `index` moved one place by `step` (−1 up, 1 down); unchanged at an end. */
export function moved(pins: Place[], index: number, step: -1 | 1): number[] {
  const ids = pins.map((pin) => pin.id);
  const to = index + step;
  if (to < 0 || to >= ids.length) return ids;
  const [id] = ids.splice(index, 1);
  ids.splice(to, 0, id as number);
  return ids;
}
