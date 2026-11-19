/** Where a kept place leads, which place an address is, and the order a pin moves to. */
import type { Place, PlaceRef } from "../../api/types/places";

/** The address a place leads to. Views and pages have no page of their own yet, so they lead to their project. */
export function placePath(place: Place): string {
  if (place.kind === "card") return `/board/${place.repository_id}?card=${place.key}`;
  if (place.kind === "project") return `/board/${place.repository_id}`;
  return `/projects/${place.repository_id}`;
}

/** The place an address is, if it is one: a project's board or page, or a card open on a board. */
export function placeAt(pathname: string, search: string): PlaceRef | null {
  const match = /^\/(?:board|projects)\/(\d+)(?:\/|$)/.exec(pathname);
  if (!match) return null;
  const card = new URLSearchParams(search).get("card");
  if (card !== null && /^\d+$/.test(card)) return { kind: "card", key: card };
  return { kind: "project", repository_id: Number(match[1]) };
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
