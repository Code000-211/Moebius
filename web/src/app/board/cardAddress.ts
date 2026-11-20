//! Where a card is in an address: `?card=<id>` on any page, which opens it as the window over that page.

/** The address's query with `card` set, or taken out when `id` is null; everything else in it stays. */
export function withCard(search: URLSearchParams, id: number | null): URLSearchParams {
  const next = new URLSearchParams(search);
  if (id === null) next.delete("card");
  else next.set("card", String(id));
  return next;
}
