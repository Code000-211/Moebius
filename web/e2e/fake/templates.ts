/**
 * The templates and the Jira import of the fake server, with the JSON and the refusals of the real one
 * (docs/reference/desktop-api.md, "Templates" and "Importing from Jira"): the issue template of each kind (the kinds
 * are those of `fake/kinds.ts`) and the page templates of the wiki, `PUT /v1/secrets/{name}` for a Jira token (a name starting `jira-`; the Models page's keys stay with its own fake), and
 * `POST /v1/projects/{id}/import/jira`, which answers a fixed report for a site it can reach and `502` for one whose
 * address says `unreachable`. A template is kept in memory; the real server commits it to the harness configuration
 * repository. The wiki fake makes a page from a page template (`POST /v1/wiki/from-template`) through `pageTemplate`.
 */
import type { IncomingMessage, ServerResponse } from "node:http";
import type { KindsFake } from "./kinds";

type Obj = Record<string, unknown>;

interface IssueTemplate {
  kind: string;
  title: string;
  body: string;
  fields: Record<string, string | number>;
}

export interface TemplatesFake {
  /** Answers one of these routes and returns true, or returns false for any other path. */
  handle(request: IncomingMessage, response: ServerResponse, path: string): Promise<boolean>;
  /** The text of a page template, or undefined. */
  pageTemplate(name: string): string | undefined;
  /** The template of a kind (in any case), or undefined. */
  issueTemplate(kind: string): { title: string; body: string } | undefined;
  /** The keys `PUT /v1/secrets/{name}` stored, by name. */
  readonly secrets: Record<string, string>;
  /** Every request made to these routes, as method, path and body (a secret's value is never kept). */
  readonly calls: Array<{ method: string; path: string; body: unknown }>;
  /** Whether the server has a harness configuration repository: without one nothing is kept. */
  editable: boolean;
}

function json(response: ServerResponse, status: number, body: unknown): true {
  const isText = typeof body === "string";
  response.writeHead(status, { "content-type": isText ? "text/plain" : "application/json" });
  response.end(isText ? body : JSON.stringify(body));
  return true;
}

function readBody(request: IncomingMessage): Promise<Obj> {
  return new Promise((done) => {
    const chunks: Buffer[] = [];
    request.on("data", (chunk: Buffer) => chunks.push(chunk));
    request.on("end", () => {
      const text = Buffer.concat(chunks).toString("utf8");
      done(text === "" ? {} : (JSON.parse(text) as Obj));
    });
  });
}

let commits = 0;
const commit = () => {
  commits += 1;
  return commits.toString(16).padStart(7, "0");
};

export function createTemplatesFake(kinds: KindsFake): TemplatesFake {
  const issues: IssueTemplate[] = [];
  const pages: Array<{ name: string; text: string }> = [];
  const secrets: Record<string, string> = {};
  const calls: TemplatesFake["calls"] = [];
  const fake: TemplatesFake = {
    secrets,
    calls,
    editable: true,
    pageTemplate: (name) => pages.find((p) => p.name === name)?.text,
    issueTemplate: (kind) => issues.find((t) => t.kind.toLowerCase() === kind.toLowerCase()),
    async handle(request, response, path) {
      const secret = /^\/v1\/secrets\/(jira-[^/]+)$/.exec(path);
      const importing = /^\/v1\/projects\/(\d+)\/import\/jira$/.exec(path);
      if (!path.startsWith("/v1/templates") && !secret && !importing) return false;
      const method = request.method ?? "GET";
      const body = method === "GET" || method === "DELETE" ? {} : await readBody(request);
      if (secret && method === "PUT") {
        // Only the name is kept for the test to read: the server never answers a key.
        secrets[decodeURIComponent(secret[1] ?? "")] = String(body.value ?? "");
        calls.push({ method, path, body: { value: "…" } });
        return json(response, 200, null);
      }
      calls.push({ method, path, body });
      if (importing) {
        if (method !== "POST") return json(response, 405, "no such route");
        const project = String(body.project ?? "");
        if (!/^[A-Za-z][A-Za-z0-9_]*$/.test(project))
          return json(response, 422, `${JSON.stringify(project)} is not a Jira project key`);
        const named = String(body.token_secret ?? "");
        if (!named.startsWith("jira-"))
          return json(
            response,
            422,
            "the token_secret of an import starts with jira-, such as jira-token",
          );
        const url = String(body.url ?? "");
        if (!/^https?:\/\//.test(url))
          return json(response, 422, `${JSON.stringify(url)} is not an http or https address`);
        if (!(named in secrets))
          return json(
            response,
            422,
            `no secret named ${named}: store the Jira token first with PUT /v1/secrets/{name}`,
          );
        if (url.includes("unreachable"))
          return json(response, 502, "Jira could not be reached: connection refused");
        return json(response, 200, {
          issues: 3,
          comments: 5,
          kinds: 2,
          people: body.users === "names" ? 0 : 2,
          cycles: 1,
          milestones: { created: 1, updated: 0, unchanged: 0 },
          links: { added: 2, removed: 0, unchanged: 0, skipped: 1 },
          warnings: [
            `${project}-9: the link to ${project}-77 is skipped, that issue is not imported`,
          ],
        });
      }
      if (path === "/v1/templates" && method === "GET")
        return json(response, 200, { editable: fake.editable, issues, pages });
      if (!fake.editable) {
        return json(response, 422, "this server has no harness configuration repository");
      }
      const issue = /^\/v1\/templates\/issues\/([^/]+)$/.exec(path);
      if (issue) {
        const name = decodeURIComponent(issue[1] ?? "");
        const kind = kinds.list().find((k) => k.name.toLowerCase() === name.toLowerCase());
        if (!kind) return json(response, 404, `no kind named ${name}`);
        const at = issues.findIndex((t) => t.kind.toLowerCase() === name.toLowerCase());
        if (method === "DELETE") {
          if (at < 0) return json(response, 404, `the kind ${kind.name} has no template`);
          issues.splice(at, 1);
          return json(response, 200, { commit: commit() });
        }
        if (method === "PUT") {
          const fields = (body.fields ?? {}) as Record<string, string | number>;
          for (const field of Object.keys(fields)) {
            if (!kind.fields.some((f) => f.name.toLowerCase() === field.toLowerCase()))
              return json(response, 422, `the kind ${kind.name} has no field ${field}`);
          }
          const made: IssueTemplate = {
            kind: kind.name,
            title: String(body.title ?? ""),
            body: String(body.body ?? ""),
            fields,
          };
          if (at < 0) issues.push(made);
          else issues[at] = made;
          return json(response, 200, { ...made, commit: commit() });
        }
      }
      const page = /^\/v1\/templates\/pages\/([^/]+)$/.exec(path);
      if (page) {
        const name = decodeURIComponent(page[1] ?? "");
        const at = pages.findIndex((t) => t.name === name);
        if (method === "DELETE") {
          if (at < 0) return json(response, 404, `no page template ${name}`);
          pages.splice(at, 1);
          return json(response, 200, { commit: commit() });
        }
        if (method === "PUT") {
          if (!/^[A-Za-z0-9_-]{1,64}$/.test(name))
            return json(response, 422, "a template name is letters, digits, - and _, up to 64");
          const made = { name, text: String(body.text ?? "") };
          if (at < 0) pages.push(made);
          else pages[at] = made;
          return json(response, 200, { ...made, commit: commit() });
        }
      }
      return json(response, 405, "no such route");
    },
  };
  return fake;
}
