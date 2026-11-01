/** Types of the calls about repositories and their files; they follow docs/reference/desktop-api.md. */
import type { Forge } from "./board";

/** The body of `POST /v1/repositories`. */
export interface NewRepository {
  name: string;
  url: string;
  forge: Forge;
  github?: string;
  project?: string;
  trusted?: string[];
}

export interface TreeEntry {
  name: string;
  path: string;
  kind: "dir" | "file";
  size: number;
}

/** `GET /v1/repositories/{id}/tree`. */
export interface Tree {
  path: string;
  entries: TreeEntry[];
}

/** `GET /v1/repositories/{id}/files/{path}`: `text` for a file of text, `binary` or `too_large` instead for others. */
export interface FileDetail {
  path: string;
  size: number;
  text?: string;
  binary?: boolean;
  too_large?: boolean;
}

export interface SearchHit {
  line: number;
  text: string;
}

export interface SearchFile {
  path: string;
  hits: SearchHit[];
}

/** `GET /v1/repositories/{id}/search`. */
export interface Search {
  files: SearchFile[];
  total: number;
  truncated: boolean;
}
