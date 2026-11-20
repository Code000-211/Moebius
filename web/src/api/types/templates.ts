/** Types of the calls about templates and the Jira import; they follow docs/reference/desktop-api.md. */
import type { FieldValue } from "./kinds";

/** The template of a kind of issue (`GET /v1/templates`): what a new issue of the kind starts with. */
export interface IssueTemplate {
  kind: string;
  title: string;
  body: string;
  fields: Record<string, FieldValue>;
}

/** A template of wiki pages. */
export interface PageTemplate {
  name: string;
  text: string;
}

/** Every template; `editable` is false on a server without a harness configuration repository. */
export interface Templates {
  editable: boolean;
  issues: IssueTemplate[];
  pages: PageTemplate[];
}

/** What `PUT /v1/templates/issues/{kind}` takes. */
export interface IssueTemplateBody {
  title: string;
  body: string;
  fields: Record<string, FieldValue>;
}

/** The body of `POST /v1/projects/{id}/import/jira`. */
export interface JiraImport {
  url: string;
  project: string;
  token_secret: string;
  email?: string;
  users?: "people" | "names";
  user_map?: Record<string, string>;
}

/** What an import made, found, and could not make. */
export interface JiraReport {
  issues: number;
  comments: number;
  kinds: number;
  people: number;
  cycles: number;
  milestones: { created: number; updated: number; unchanged: number };
  links: { added: number; removed: number; unchanged: number; skipped: number };
  warnings: string[];
}
