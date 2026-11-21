/**
 * Where the person is in the app, in a few lines for the Moebius agent (the former desktop app's `page_context`): the page,
 * the repository of a board, the chat that is open. The Moebius agent's sheet sends it with every message; the agent is told
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

export function pageContext(
  pathname: string,
  snapshot: Snapshot | undefined,
  search = "",
): PageContext {
  const parts = pathname.split("/").filter(Boolean);
  const [first, second, third, fourth] = parts;
  const boards = snapshot?.boards ?? [];
  if (first === "projects" && /^\d+$/.test(second ?? "")) {
    const projectId = Number(second);
    const repositoryNamed =
      third === "repositories"
        ? Number(fourth)
        : Number(new URLSearchParams(search).get("repository"));
    const board =
      boards.find((each) => each.repository.id === repositoryNamed) ??
      boards.find((each) => each.repository.home_project_id === projectId);
    const project = board?.repository;
    // The board of a project, its list, or a card on it.
    if (third === undefined || third === "list") {
      const lines = ["Page: Projects > Board"];
      if (project) lines.push(`Repository: ${project.name} (id ${project.id})`);
      return {
        text: lines.join("\n"),
        name: project ? project.name : "Board",
        repository: project ? project.id : null,
      };
    }
    const lines = ["Page: Projects > Project"];
    if (project) lines.push(`Repository: ${project.name} (id ${project.id})`);
    // Without this the agent takes a request on this page to be about the repository's code.
    lines.push(
      "This page is the project's settings (rules, checks, scripts and what plugins ask of it): a request made here is about those settings, not the code of the repository.",
    );
    return { text: lines.join("\n"), name: project ? project.name : "Project", repository: null };
  }
  const page = first === undefined ? undefined : pageByPath(first);
  const lines = [
    page
      ? page.group && page.group !== "Other"
        ? `Page: ${page.group} > ${page.title}`
        : `Page: ${page.title}`
      : `Page: ${pathname}`,
  ];
  if (first === "settings" || first === "plugin-settings") {
    lines.push(
      "This page sets up Moebius itself (its settings, models and plugins): a request made here is about that setup, not the code of a repository.",
    );
  }
  if (first === "chat" && second !== undefined) lines.push(`Open chat: ${second}`);
  return { text: lines.join("\n"), name: page ? page.title : pathname, repository: null };
}
