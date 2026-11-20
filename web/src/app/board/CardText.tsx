/**
 * The text of a card, its body or a comment, as Markdown with its `[[Page]]` and `[[Page|label]]` links made links to
 * the wiki's pages, as in the wiki itself (`docs/reference/desktop-api.md`, "Links, backlinks and renames"): a card's
 * text leads to pages as if it were in its project's folder. A link to no page stays as written. The pages are read
 * only for a text that has a link in it.
 */
import { useQuery } from "@tanstack/react-query";
import { wikiPages } from "../../api/calls/wiki";
import { keys } from "../../api/keys";
import { Markdown } from "../../ui/chat/Markdown";
import { linkPages, pageLinksOf } from "../wiki/links";

export function CardText({
  content,
  projectKey,
  className,
}: {
  content: string;
  projectKey: string | undefined;
  className?: string;
}) {
  const linked = content.includes("[[");
  const pages = useQuery({
    queryKey: keys.wikiPages,
    queryFn: wikiPages,
    enabled: linked,
    retry: false,
  });
  const text = linked
    ? linkPages(content, pageLinksOf(content, pages.data ?? [], projectKey))
    : content;
  return <Markdown content={text} keepNewlines className={className} />;
}
