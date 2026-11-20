/**
 * What links to a page or a card: the pages and cards that link to a page (`Backlinks`, under the outline at the right
 * of the Wiki page), and the pages that link to a card (`WikiPagesProperty`, in the properties of a
 * card). Each is a link that opens the page or the card. A wiki the server does not have, or a read that fails, shows
 * nothing: these are an addition to what the page already says.
 */
import { useQuery } from "@tanstack/react-query";
import { Link } from "react-router";
import { wikiCardPages } from "../../api/calls/wiki";
import { keys } from "../../api/keys";
import type { PageLinks } from "../../api/types/wiki";
import { Property } from "../../ui/page/Properties";
import { wikiAddress } from "./links";

const LINK =
  "min-h-7 truncate rounded-control text-sm text-text underline-offset-4 hover:underline focus-visible:underline";
/** The same links in the outline column: the column has the room of its own, so the row is the outline's. */
const OUTLINE_LINK =
  "flex min-h-7 items-center truncate rounded-control px-2 text-sm text-muted hover:bg-hover hover:text-text focus-visible:text-text";

/** The pages and cards that link to the page; nothing while there are none. */
export function Backlinks({ links }: { links: PageLinks | undefined }) {
  const pages = links?.backlinks.pages ?? [];
  const cards = links?.backlinks.cards ?? [];
  if (pages.length === 0 && cards.length === 0) return null;
  return (
    <section className="flex flex-col border-t border-hairline pt-3">
      <h2 className="m-0 px-2 pb-1 text-xs font-medium text-muted">Linked from</h2>
      <ul aria-label="Linked from" className="m-0 flex list-none flex-col p-0">
        {pages.map((page) => (
          <li key={`page:${page.path}`} className="min-w-0">
            <Link to={wikiAddress(page.path)} className={OUTLINE_LINK} title={page.path}>
              {page.name}
            </Link>
          </li>
        ))}
        {cards.map((card) => (
          <li key={`card:${card.key}`} className="min-w-0">
            <Link to={`/card/${card.key}`} className={OUTLINE_LINK} title={card.title ?? undefined}>
              {card.key}
              {card.title ? ` · ${card.title}` : ""}
            </Link>
          </li>
        ))}
      </ul>
    </section>
  );
}

/** The pages of the wiki that link to the card `cardName` (`WEB-12`); nothing while there are none. */
export function WikiPagesProperty({ cardName }: { cardName: string }) {
  const pages = useQuery({
    queryKey: keys.wikiCardPages(cardName),
    queryFn: () => wikiCardPages(cardName),
    retry: false,
  });
  if (!pages.data || pages.data.length === 0) return null;
  return (
    <Property name="Wiki">
      <ul
        aria-label="Pages that link to the card"
        className="m-0 flex list-none flex-col gap-0.5 p-0"
      >
        {pages.data.map((page) => (
          <li key={page.path} className="min-w-0">
            <Link to={wikiAddress(page.path)} className={LINK} title={page.path}>
              {page.name}
            </Link>
          </li>
        ))}
      </ul>
    </Property>
  );
}
