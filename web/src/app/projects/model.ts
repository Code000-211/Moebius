/**
 * The logic of the Projects pages, apart from React: how a repository is told in a list, what the form of a new one
 * sends, the logins a person types, what the editor says of a file it cannot edit, and what the server's answer to a
 * refused repository means to the person who asked for it.
 */
import type { Forge, Repository } from "../../api/types/board";
import type { FileDetail, NewRepository } from "../../api/types/projects";
import { plural } from "../../ui/lib/format";

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
  return plural(count, "card", "cards");
}

/** The field an answer about a new repository belongs to. */
export type FailureField = "name" | "url";

/**
 * What the server said about a repository it refused, told to the person who asked for it. The headline is what
 * happened, `next` is what to do about it when there is something to do, and `detail` is the exact sentence the server
 * gave, kept for someone who wants it rather than shown as the headline.
 */
export interface Failure {
  headline: string;
  next: string;
  /** Null when the whole form is the problem rather than one field of it. */
  field: FailureField | null;
  detail: string | null;
}

/** What git printed inside `git … failed: …`, or null when the sentence is not the failure of a run. */
function gitSaid(message: string): string | null {
  if (!message.startsWith("git ")) return null;
  const at = message.indexOf(" failed: ");
  return at === -1 ? null : message.slice(at + " failed: ".length).trim();
}

/**
 * The service the address or the error names — `GitHub`, `GitLab`, `Forgejo` — or null when it names none. A refused
 * clone and a refused issue are both about a forge the server was never told how to reach.
 */
export function forgeNamed(message: string): string | null {
  const text = message.toLowerCase();
  if (text.includes("github")) return "GitHub";
  if (text.includes("gitlab")) return "GitLab";
  if (text.includes("codeberg") || text.includes("forgejo") || text.includes("gitea"))
    return "Forgejo";
  return null;
}

/** Whether the address asks for a sign-in over HTTPS, which is where a token belongs. */
function overHttps(message: string): boolean {
  return message.toLowerCase().includes("https://");
}

const SIGN_IN = [
  "could not read username",
  "could not read password",
  "terminal prompts disabled",
  "authentication failed",
  "access denied",
  "invalid credentials",
  "bad credentials",
  "permission denied (publickey)",
  "could not read from remote repository",
];

const MISSING = ["not found", "does not exist", "404"];

const UNREACHABLE = [
  "could not resolve host",
  "name or service not known",
  "connection refused",
  "connection reset",
  "timed out",
  "unable to access",
  "network is unreachable",
  "certificate",
];

/**
 * Where the credential for the service named in `message` is set. Those keys are written to the configuration and only
 * read when the server starts, so the way to take one says that the server has to restart.
 */
function credentialPlace(name: string): string {
  if (name === "GitHub") return "Set up the GitHub App under Settings";
  return `Set the ${name} token under Settings`;
}

/**
 * The sentence a refused `POST /v1/repositories` carries, told in words a person can act on. The server quotes git
 * whole, so a private repository over HTTPS comes back as a `git clone --quiet -- …` command line: what happened and
 * what to do next come first here, and the exact sentence is kept in `detail`. A sentence that is already plain — a
 * name that is taken, a folder that is not a repository — is passed through as it came.
 */
export function addFailure(message: string): Failure {
  const said = gitSaid(message);
  if (said === null) {
    if (message.startsWith("cannot run git:")) {
      return {
        headline: "The server cannot run git.",
        next: "Install git where the server runs, then try again.",
        field: null,
        detail: message,
      };
    }
    const field: FailureField = /name|exists/i.test(message) ? "name" : "url";
    return { headline: message, next: "", field, detail: null };
  }

  const saidText = said.toLowerCase();
  const name = forgeNamed(message);
  const base = { field: "url" as const, detail: message };
  if (SIGN_IN.some((sign) => saidText.includes(sign))) {
    return {
      headline: name ? `The server cannot sign in to ${name}.` : "The server cannot sign in here.",
      next: name
        ? `${credentialPlace(name)} and restart the server, then try again.`
        : "A private repository needs credentials on the server: give it a token, or use an address that needs none.",
      ...base,
    };
  }
  if (MISSING.some((sign) => saidText.includes(sign))) {
    return {
      headline: "No repository at that address.",
      next: overHttps(message)
        ? "Check the address. A private repository also reads as missing when the server cannot sign in to it."
        : "Check the address, or the path on the server.",
      ...base,
    };
  }
  if (UNREACHABLE.some((sign) => saidText.includes(sign))) {
    return {
      headline: "The server could not reach that address.",
      next: "Check the address and the network where the server runs.",
      ...base,
    };
  }
  return {
    headline: "The server could not clone this repository.",
    next: "Check the address and try again.",
    ...base,
  };
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

/** The files of a project's settings that the settings panel edits as text, with what each is for. */
export const SETTING_FILES: { file: string; label: string; hint: string }[] = [
  {
    file: "review.md",
    label: "Review notes",
    hint: "What the Standards review should also check, in Markdown.",
  },
  {
    file: "lint.toml",
    label: "Lint plan",
    hint: "What the lens checks after an agent's edits (TOML); without it the common tools are detected.",
  },
  {
    file: "checks",
    label: "Checks",
    hint: "The command that checks a change of a repository without a forge, run with sh.",
  },
  {
    file: "localize",
    label: "Files hint",
    hint: "Lines `on`, `off` and `files <n>` for the hint of likely files.",
  },
];

/** The first line of a file's text that says something, cut short, as the settings panel shows what a file holds. */
export function firstLine(text: string): string {
  const line = text.split("\n").find((each) => each.trim() !== "") ?? "(empty)";
  return line.length > 80 ? `${line.slice(0, 79)}…` : line;
}
