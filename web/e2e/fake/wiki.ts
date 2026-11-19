/**
 * The wiki of the fake server (`/v1/wiki/`, see "The wiki" in the desktop API reference): pages kept in memory with
 * every version, as the commits of the wiki's repository are, so a page can be written, moved, deleted, read as it was
 * and its history listed. Each edit sends `wiki_changed`. Only what the Wiki page uses is here, with the server's
 * answers and errors for it.
 */
import type { IncomingMessage, ServerResponse } from "node:http";

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

export function createWikiFake(send: Send): WikiFake {
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

  return {
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
