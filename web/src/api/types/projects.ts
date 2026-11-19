/** Types of the calls about projects, repositories and their files; they follow docs/reference/desktop-api.md. */
import type { Forge } from "./board";

/** A project as `GET /v1/projects` answers it (`ProjectView`): a board over its repositories, with a key for good. */
export interface Project {
  id: number;
  name: string;
  /** Such as `WEB`, which names the project's cards; kept when the project is renamed. */
  key: string;
  created_at: number;
  /** In the order the project names them; `home` when this project is where the repository's new issues land. */
  repositories: Array<{ id: number; name: string; home: boolean }>;
  /** Everyone who is not disabled, while no project is private. */
  members: Array<{ id: number; handle: string; name: string }>;
  /** The cards on its board that are neither closed nor in Done. */
  open_cards: number;
  /** The repository that holds the project's own files, whose board is the project's when it has no repository. */
  files: number | null;
}

/** The body of `POST /v1/projects`. */
export interface NewProject {
  name: string;
  key: string;
}

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

/** One file of a project's settings (`GET /v1/repositories/{id}/settings`, and the answer of a `PUT` or `DELETE`). */
export interface SettingFile {
  file: string;
  exists: boolean;
  text: string;
  /** The short id of the commit a save or a removal made; left out when it changed nothing. */
  commit?: string;
}

/** `GET /v1/repositories/{id}/settings`: the files of `repos/<name>/` in the harness configuration repository. */
export interface ProjectSettings {
  path: string;
  /** False when the server has no harness configuration repository: nothing can be saved. */
  editable: boolean;
  files: SettingFile[];
  /** The column scripts the project has of its own, such as `ready.luau`. */
  scripts: string[];
  /** The files of a `.moebius/` folder the default branch still has, which are not read. */
  leftover: string[];
}

/** One setting a plugin asks of every repository or of the server, with its value (empty when it has none). */
export interface PluginSetting {
  key: string;
  label: string;
  description: string;
  /** `line`: a field of one line; `code`: an editor for a file. */
  input: "line" | "code";
  /** For `code`, the language to highlight, such as `dockerfile`. */
  language?: string;
  value: string;
}

/** The settings of one plugin in one scope. */
export interface PluginSettings {
  plugin: string;
  version: string;
  settings: PluginSetting[];
  /** False for a plugin that is turned off: its settings are kept and can be changed. */
  enabled: boolean;
  /** Whether the plugin brings its own page for these settings (`GET /v1/plugins/{plugin}/settings-ui`). */
  ui: boolean;
}

/** `GET /v1/repositories/{id}/plugin-settings`: every running plugin that has project settings. */
export interface PluginProjectSettings {
  repository: number;
  plugins: PluginSettings[];
}

/** `GET /v1/plugins/settings`: every running plugin that has app settings. */
export interface AppSettings {
  plugins: PluginSettings[];
}
