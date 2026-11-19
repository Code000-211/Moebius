/** The calls of the Wiki page: the pages, one page (or an older version of it), its history, and writing, moving and deleting one. */
import { del, get, post, put } from "../http";
import type { PageInfo, PageLinks, Version, WikiHit, WikiPageView } from "../types/wiki";

/** A page's path in a URL: each part percent-encoded, the slashes kept. */
export function pageUrl(path: string): string {
  return path.split("/").map(encodeURIComponent).join("/");
}

/** Every page, sorted by path. */
export function wikiPages(): Promise<PageInfo[]> {
  return get<PageInfo[]>("/v1/wiki/pages");
}

/** The page `path`, or with `at` as it was after that commit of its history. */
export function wikiPage(path: string, at?: string): Promise<WikiPageView> {
  const query = at ? `?at=${encodeURIComponent(at)}` : "";
  return get<WikiPageView>(`/v1/wiki/pages/${pageUrl(path)}${query}`);
}

/** Writes the page `path`, making it if it is not there: a commit by the person, pushed to the wiki's origin. */
export function writeWikiPage(path: string, text: string, message?: string): Promise<WikiPageView> {
  return put<WikiPageView>(
    `/v1/wiki/pages/${pageUrl(path)}`,
    message ? { text, message } : { text },
  );
}

/** Moves or renames a page; its history follows it. */
export function moveWikiPage(from: string, to: string): Promise<WikiPageView> {
  return post<WikiPageView>("/v1/wiki/moves", { from, to });
}

/** Deletes a page; its history stays. */
export async function deleteWikiPage(path: string): Promise<void> {
  await del<unknown>(`/v1/wiki/pages/${pageUrl(path)}`);
}

/** The commits that changed a page, newest first. */
export function wikiHistory(path: string): Promise<Version[]> {
  return get<Version[]>(`/v1/wiki/history/${pageUrl(path)}`);
}

/** Merges the wiki proposal of inbox item `item`: the page changes and the item resolves. */
export async function mergeWikiProposal(item: number): Promise<void> {
  await post<unknown>(`/v1/wiki/proposals/${item}/merge`, {});
}

/** Dismisses the wiki proposal of inbox item `item`; the wiki stays as it is. */
export async function dismissWikiProposal(item: number): Promise<void> {
  await post<unknown>(`/v1/wiki/proposals/${item}/dismiss`, {});
}

/** The pages found by words of their title and text, the best first; with `project`, only that project's pages. */
export function searchWiki(query: string, project?: string, limit?: number): Promise<WikiHit[]> {
  const params = new URLSearchParams({ q: query });
  if (project) params.set("project", project);
  if (limit) params.set("limit", String(limit));
  return get<WikiHit[]>(`/v1/wiki/search?${params}`);
}

/** What the page links to and what links to it, pages and cards. */
export function wikiLinks(path: string): Promise<PageLinks> {
  return get<PageLinks>(`/v1/wiki/links/${pageUrl(path)}`);
}

/** The pages that link to the card named `key`, such as `WEB-12`. */
export function wikiCardPages(key: string): Promise<PageInfo[]> {
  return get<PageInfo[]>(`/v1/wiki/cards/${encodeURIComponent(key)}`);
}
