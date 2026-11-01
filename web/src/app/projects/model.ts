/**
 * The logic of the Projects pages, apart from React: how a repository is told in a list, what the form of a new one
 * sends, the logins a person types, and what the editor says of a file it cannot edit.
 */
import type { Forge, Repository } from "../../api/types/board";
import type { FileDetail, NewRepository } from "../../api/types/projects";

/** The forge and the repository's name there, as a page shows it: `GitHub · acme/radiance`. */
export function forgeTag(repository: Repository): string {
  if (repository.forge === "gitlab" && repository.project) return `GitLab · ${repository.project}`;
  if (repository.forge === "forgejo" && repository.project)
    return `Forgejo · ${repository.project}`;
  if (repository.github) return `GitHub · ${repository.github}`;
  return "No forge";
}

/** Whether the repository is on a forge, whose logins it may trust. */
export function onForge(repository: Repository): boolean {
  return repository.github !== null || Boolean(repository.project);
}

export function cardsText(count: number): string {
  return count === 1 ? "1 card" : `${count} cards`;
}

/** The logins typed, separated by commas, spaces or lines. */
export function logins(typed: string): string[] {
  return typed
    .split(/[,\s]+/)
    .map((login) => login.trim())
    .filter((login) => login !== "");
}

/** What the form of a new repository holds. */
export interface RepositoryForm {
  name: string;
  url: string;
  forge: Forge;
  github: string;
  project: string;
  trusted: string;
}

export const EMPTY_FORM: RepositoryForm = {
  name: "",
  url: "",
  forge: "none",
  github: "",
  project: "",
  trusted: "",
};

/** What the form asks the server for, when it is filled in. */
export function repositoryRequest(form: RepositoryForm): NewRepository | null {
  const name = form.name.trim();
  const url = form.url.trim();
  const github = form.github.trim();
  const project = form.project.trim();
  const forgeName = form.forge === "none" ? "-" : form.forge === "github" ? github : project;
  if (name === "" || url === "" || forgeName === "") return null;
  const request: NewRepository = { name, url, forge: form.forge };
  if (form.forge === "github") request.github = github;
  if (form.forge === "gitlab" || form.forge === "forgejo") request.project = project;
  if (form.forge !== "none") request.trusted = logins(form.trusted);
  return request;
}

/** What the editor says of a file it does not edit, or null for a file of text. */
export function notEditable(file: FileDetail): string | null {
  if (file.binary) return `${file.path} is binary (${file.size} bytes), not editable here.`;
  if (file.too_large) {
    return `${file.path} is over 1 MB (${file.size} bytes), not editable here.`;
  }
  return null;
}

/** The directories above a path, outermost first: `a/b/c.rs` has `a` and `a/b`. */
export function parents(path: string): string[] {
  const parts = path.split("/").slice(0, -1);
  return parts.map((_, at) => parts.slice(0, at + 1).join("/"));
}

/** The 0-based offset in `text` where the 1-based `line` starts; the end of the text for a line past it. */
export function lineStart(text: string, line: number): number {
  let offset = 0;
  for (let at = 1; at < line; at += 1) {
    const next = text.indexOf("\n", offset);
    if (next === -1) return text.length;
    offset = next + 1;
  }
  return offset;
}
