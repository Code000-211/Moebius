/**
 * Where the person is in the app, in a few lines for the Moebius agent (the former desktop app's `page_context`): the page,
 * the repository of a board, the chat that is open. The floating chat sends it with every message; the agent is told
 * it before the message, and the conversation does not show it.
 */

import type { Snapshot } from "../api/types/board";
import { pageByPath } from "../app/pages";

export interface PageContext {
  /** The lines for the agent. */
  text: string;
  /** The page in the few words a person would call it by: the board's repository, or the page's own name. */
  name: string;
  /** The repository of the page, if it is a board's: a chat started there works in it. */
  repository: number | null;
}

export function pageContext(pathname: string, snapshot: Snapshot | undefined): PageContext {
  const parts = pathname.split("/").filter(Boolean);
  const [first, second] = parts;
  if (first === "board" && second !== undefined) {
    const id = Number(second);
    const board = snapshot?.boards.find((each) => each.repository.id === id);
    const lines = ["Page: Projects > Board"];
    if (board) {
      lines.push(`Repository: ${board.repository.name} (id ${board.repository.id})`);
    }
    return {
      text: lines.join("\n"),
      name: board ? board.repository.name : "Board",
      repository: board ? id : null,
    };
  }
  const page = first === undefined ? undefined : pageByPath(first);
  const lines = [
    page
      ? page.group
        ? `Page: ${page.group} > ${page.title}`
        : `Page: ${page.title}`
      : `Page: ${pathname}`,
  ];
  if (first === "chat" && second !== undefined) lines.push(`Open chat: ${second}`);
  return { text: lines.join("\n"), name: page ? page.title : pathname, repository: null };
}
