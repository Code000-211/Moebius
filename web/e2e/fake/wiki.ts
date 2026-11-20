/**
 * The wiki of the fake server (`/v1/wiki/`, see "The wiki" in the desktop API reference): pages kept in memory with
 * every version, as the commits of the wiki's repository are, so a page can be written, moved, deleted, read as it was
 * and its history listed. Each edit sends `wiki_changed`. Only what the Wiki page uses is here, with the server's
 * answers and errors for it.
 */
import type { IncomingMessage, ServerResponse } from "node:http";
import type { PagesAFake } from "./pages-a";
import type { TemplatesFake } from "./templates";

type Send = (event: { kind: string; data: unknown }) => void;

interface Version {
  commit: string;
  author: string;
  handle: string | null;
  at: number;
  message: string;
  path: string;
  /** The text after the commit; null when the commit deleted the page. */
  text: string | null;
}

export interface WikiFake {
  /** Answers a route of the wiki and returns true, or returns false for any other path. */
  handle(
    request: IncomingMessage,
    response: ServerResponse,
    path: string,
    author: { name: string; handle: string },
  ): Promise<boolean>;
  /**
   * An agent's edit of a page, as `POST /v1/wiki/proposals` makes it: nothing changes, and the inbox gets an item of
   * the kind `wiki_proposal` whose `diff` is the change. Merging it (`POST /v1/wiki/proposals/{item}/merge`) writes the
   * page and dismissing it leaves the wiki as it is; either resolves the item.
   */
  propose(path: string, text: string, message: string): number;
  /** A page as someone else pushed it to the wiki's repository, which the next read shows. */
  pushFromElsewhere(path: string, text: string): void;
}

function readBody(request: IncomingMessage): Promise<string> {
  return new Promise((done) => {
    const chunks: Buffer[] = [];
    request.on("data", (chunk: Buffer) => chunks.push(chunk));
    request.on("end", () => done(Buffer.concat(chunks).toString("utf8")));
  });
}

function answer(response: ServerResponse, status: number, value: unknown): void {
  const isText = typeof value === "string";
  response.writeHead(status, { "content-type": isText ? "text/plain" : "application/json" });
  response.end(isText ? value : JSON.stringify(value));
}

/** A page path as the URL carries it: percent-encoded parts. */
const decoded = (rest: string) => rest.split("/").map(decodeURIComponent).join("/");

/** Answers `status` with `message` and says the route was answered. */
function refuse(response: ServerResponse, status: number, message: string): true {
  answer(response, status, message);
  return true;
}

const nameOf = (path: string) => path.split("/").at(-1) ?? path;

/** The pages a text links to by `[[name]]` or `[[path|label]]`, as the server resolves them: by path, else by name. */
function pageLinks(text: string, all: string[]): Array<{ link: string; path: string | null }> {
  return [...text.matchAll(/\[\[([^[\]|\n]+)(?:\|[^[\]\n]*)?\]\]/g)].map((found) => {
    const link = (found[1] ?? "").trim();
    const wanted = link.toLowerCase();
    const path =
      all.find((each) => each.toLowerCase() === wanted) ??
      all.find((each) => nameOf(each).toLowerCase() === wanted) ??
      null;
    return { link, path };
  });
}

export function createWikiFake(send: Send, pagesA: PagesAFake, templates: TemplatesFake): WikiFake {
  /** Every page that ever was, by its path now, with its versions oldest first. */
  const pages = new Map<string, Version[]>();
  let commits = 0;
  let clock = 1_790_000_000;
  const commit = () => {
    commits += 1;
    clock += 60;
    return commits.toString(16).padStart(40, "a");
  };
  const current = (path: string) => {
    const last = pages.get(path)?.at(-1);
    return last && last.text !== null ? last : undefined;
  };
  const view = (path: string, version: Version) => ({
    path,
    name: nameOf(path),
    text: version.text ?? "",
    version: { ...version, text: undefined },
  });
  const record = (path: string, version: Version) => {
    const versions = pages.get(path) ?? [];
    versions.push(version);
    pages.set(path, versions);
    send({ kind: "wiki_changed", data: { commit: version.commit } });
  };
  const history = (path: string) =>
    [...(pages.get(path) ?? [])].reverse().map((version) => ({ ...version, text: undefined }));

  /** The proposals that wait, by the id of their inbox item. */
  const proposals = new Map<
    number,
    { path: string; text: string; message: string; item: { resolved_at: number | null } }
  >();
  const diffOf = (path: string, before: string, after: string) => {
    const lines = (text: string) => (text === "" ? [] : text.replace(/\n$/, "").split("\n"));
    return [
      `diff --git a/${path}.md b/${path}.md`,
      `--- a/${path}.md`,
      `+++ b/${path}.md`,
      "@@",
      ...lines(before).map((line) => `-${line}`),
      ...lines(after).map((line) => `+${line}`),
      "",
    ].join("\n");
  };

  return {
    propose(path, text, message) {
      const before = current(path)?.text ?? "";
      const item = pagesA.addInboxItem({
        kind: "wiki_proposal",
        title: `Wiki: ${message}`,
        data: {
          branch: `proposal/run-1-${proposals.size + 1}`,
          page: path,
          message,
          by: "Coder",
          commit: "b".repeat(40),
          diff: diffOf(path, before, text),
        },
      });
      proposals.set(item.id, { path, text, message, item });
      return item.id;
    },
    pushFromElsewhere(path, text) {
      record(path, {
        commit: commit(),
        author: "Grace",
        handle: null,
        at: clock,
        message: "Edit elsewhere",
        path,
        text: text.endsWith("\n") ? text : `${text}\n`,
      });
    },
    async handle(request, response, path, author) {
      if (!path.startsWith("/v1/wiki/")) return false;
      const method = request.method ?? "GET";
      const query = new URL(request.url ?? "/", "http://x").searchParams;
      if (path === "/v1/wiki/pages" && method === "GET") {
        const listed = [...pages.keys()]
          .filter((each) => current(each))
          .sort()
          .map((each) => ({ path: each, name: nameOf(each) }));
        answer(response, 200, listed);
        return true;
      }
      if (path === "/v1/wiki/search" && method === "GET") {
        const words = (query.get("q") ?? "").toLowerCase().split(/\s+/).filter(Boolean);
        const hits = [...pages.keys()]
          .filter((each) => current(each))
          .map((each) => ({ each, text: current(each)?.text ?? "" }))
          .filter(({ each, text }) =>
            words.every((word) => `${nameOf(each)} ${text}`.toLowerCase().includes(word)),
          )
          .sort(
            (a, b) =>
              Number(words.every((w) => nameOf(b.each).toLowerCase().includes(w))) -
                Number(words.every((w) => nameOf(a.each).toLowerCase().includes(w))) ||
              a.each.localeCompare(b.each),
          )
          .map(({ each, text }) => ({
            path: each,
            name: nameOf(each),
            project: each.startsWith("projects/") ? (each.split("/")[1] ?? null) : null,
            snippet: text.trim().slice(0, 80),
          }));
        answer(response, 200, hits);
        return true;
      }
      if (path === "/v1/wiki/from-template" && method === "POST") {
        const body = JSON.parse(await readBody(request)) as { template: string; path: string };
        const text = templates.pageTemplate(body.template);
        if (text === undefined) return refuse(response, 404, `no page template ${body.template}`);
        if (current(body.path)) return refuse(response, 409, `page ${body.path} exists already`);
        const made: Version = {
          commit: commit(),
          author: author.name,
          handle: author.handle,
          at: clock,
          message: `Add ${body.path} from the template ${body.template}`,
          path: body.path,
          text: text.endsWith("\n") ? text : `${text}\n`,
        };
        record(body.path, made);
        answer(response, 200, view(body.path, made));
        return true;
      }
      const decision = /^\/v1\/wiki\/proposals\/(\d+)\/(merge|dismiss)$/.exec(path);
      if (decision && method === "POST") {
        const id = Number(decision[1]);
        const proposal = proposals.get(id);
        if (!proposal || proposal.item.resolved_at !== null)
          return refuse(response, 404, `no open wiki proposal ${id}`);
        proposal.item.resolved_at = ++clock;
        if (decision[2] === "dismiss") {
          answer(response, 200, null);
          return true;
        }
        const made: Version = {
          commit: commit(),
          author: author.name,
          handle: author.handle,
          at: clock,
          message: `Merge the proposal: ${proposal.message}`,
          path: proposal.path,
          text: proposal.text.endsWith("\n") ? proposal.text : `${proposal.text}\n`,
        };
        record(proposal.path, made);
        answer(response, 200, { commit: made.commit });
        return true;
      }
      const linksRoute = /^\/v1\/wiki\/links\/(.+)$/.exec(path);
      if (linksRoute && method === "GET") {
        const page = decoded(linksRoute[1] ?? "");
        const all = [...pages.keys()].filter((each) => current(each)).sort();
        const links = pageLinks(current(page)?.text ?? "", all);
        const from = all.filter((each) =>
          pageLinks(current(each)?.text ?? "", all).some((link) => link.path === page),
        );
        answer(response, 200, {
          links: { pages: links, cards: [] },
          backlinks: {
            pages: from.map((each) => ({ path: each, name: nameOf(each) })),
            cards: [],
          },
        });
        return true;
      }
      if (path === "/v1/wiki/refresh" && method === "POST") {
        answer(response, 200, null);
        return true;
      }
      if (path === "/v1/wiki/moves" && method === "POST") {
        const body = JSON.parse(await readBody(request)) as { from: string; to: string };
        const moved = current(body.from);
        if (!moved) return refuse(response, 404, `page ${body.from} not found`);
        if (current(body.to)) return refuse(response, 409, `page ${body.to} exists already`);
        const sameFolder =
          body.from.split("/").slice(0, -1).join("/") === body.to.split("/").slice(0, -1).join("/");
        const versions = pages.get(body.from) ?? [];
        pages.delete(body.from);
        pages.set(body.to, versions);
        const made: Version = {
          commit: commit(),
          author: author.name,
          handle: author.handle,
          at: clock,
          message: sameFolder
            ? `Rename ${body.from} to ${nameOf(body.to)}`
            : `Move ${body.from} to ${body.to}`,
          path: body.to,
          text: moved.text,
        };
        record(body.to, made);
        // The links to the page in the other pages follow it, by path or by name.
        const all = [...pages.keys()].filter((each) => current(each)).sort();
        for (const other of all.filter((each) => each !== body.to)) {
          const before = current(other);
          if (!before?.text) continue;
          const text = before.text.replace(
            /\[\[([^[\]|\n]+)(\|[^[\]\n]*)?\]\]/g,
            (whole: string, link: string, label?: string) => {
              const wanted = link.trim().toLowerCase();
              const leads =
                wanted === body.from.toLowerCase() || wanted === nameOf(body.from).toLowerCase();
              if (!leads) return whole;
              return `[[${link.includes("/") ? body.to : nameOf(body.to)}${label ?? ""}]]`;
            },
          );
          if (text === before.text) continue;
          record(other, {
            commit: before.commit,
            author: author.name,
            handle: author.handle,
            at: clock,
            message: `Relink ${other}`,
            path: other,
            text,
          });
        }
        answer(response, 200, view(body.to, made));
        return true;
      }
      const historyRoute = /^\/v1\/wiki\/history\/(.+)$/.exec(path);
      if (historyRoute && method === "GET") {
        const page = decoded(historyRoute[1] ?? "");
        if (!pages.has(page)) return refuse(response, 404, `page ${page} not found`);
        answer(response, 200, history(page));
        return true;
      }
      const pageRoute = /^\/v1\/wiki\/pages\/(.+)$/.exec(path);
      if (!pageRoute) return false;
      const page = decoded(pageRoute[1] ?? "");
      if (method === "GET") {
        const at = query.get("at");
        const version = at
          ? pages.get(page)?.find((each) => each.commit.startsWith(at) && each.text !== null)
          : current(page);
        if (!version) return refuse(response, 404, `page ${page} not found`);
        answer(response, 200, view(page, version));
        return true;
      }
      if (method === "PUT") {
        const body = JSON.parse(await readBody(request)) as { text: string; message?: string };
        const text = body.text === "" || body.text.endsWith("\n") ? body.text : `${body.text}\n`;
        const before = current(page);
        if (before && before.text === text) {
          answer(response, 200, view(page, before));
          return true;
        }
        const made: Version = {
          commit: commit(),
          author: author.name,
          handle: author.handle,
          at: clock,
          message: body.message?.split("\n")[0] || `${before ? "Update" : "Add"} ${page}`,
          path: page,
          text,
        };
        record(page, made);
        answer(response, 200, view(page, made));
        return true;
      }
      if (method === "DELETE") {
        if (!current(page)) return refuse(response, 404, `page ${page} not found`);
        record(page, {
          commit: commit(),
          author: author.name,
          handle: author.handle,
          at: clock,
          message: `Delete ${page}`,
          path: page,
          text: null,
        });
        answer(response, 200, null);
        return true;
      }
      return false;
    },
  };
}
