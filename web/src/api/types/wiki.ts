/** The wiki's answers (`/v1/wiki/`): its pages, a page with the commit that last changed it, and a page's history. */

/** A page in the list of pages: its path without `.md`, and the last part of it, which it is called by. */
export interface PageInfo {
  path: string;
  name: string;
}

/** A commit that changed a page. */
export interface Version {
  commit: string;
  /** The author's name. */
  author: string;
  /** The author's handle when a person of Moebius made the commit; null for one pushed from elsewhere. */
  handle: string | null;
  /** Unix seconds. */
  at: number;
  /** The first line of the commit message. */
  message: string;
  /** The page's path in that commit, which differs from today's before a rename or a move. */
  path: string;
}

/** A page as it is, or as one commit of its history left it. */
export interface WikiPageView {
  path: string;
  name: string;
  text: string;
  version: Version;
}

/** A page a search found. */
export interface WikiHit {
  path: string;
  name: string;
  /** The key of the project the page is under; null for the organisation's pages. */
  project: string | null;
  /** The words around the match, as plain text. */
  snippet: string;
}
