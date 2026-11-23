/**
 * The calls about projects (listing and making one) and repositories: adding, the trusted logins, removing, reading and writing their files, their settings, and
 * the settings plugins ask of them.
 */
import { del, get, patch, post, put } from "../http";
import type { AgentsTake, Repository } from "../types/board";
import type {
  FileDetail,
  NewProject,
  NewRepository,
  PluginProjectSettings,
  PluginSettings,
  Project,
  ProjectSettings,
  Search,
  SettingFile,
  Tree,
} from "../types/projects";

/** Every project, in the order they were made. */
export function listProjects(): Promise<Project[]> {
  return get<Project[]>("/v1/projects");
}

/** Makes a project with no repository yet; the server refuses a key another project has. */
export function addProject(project: NewProject): Promise<Project> {
  return post<Project>("/v1/projects", project);
}

/** Renames a project (an owner); its key stays and its settings move with it. */
export function renameProject(project: number, name: string): Promise<Project> {
  return patch<Project>(`/v1/projects/${project}`, { name });
}

/** Makes a project private or public and names its members by handle (an owner); the list replaces the old one. */
export function setProjectAccess(
  project: number,
  access: { private: boolean; members: string[] },
): Promise<Project> {
  return put<Project>(`/v1/projects/${project}/access`, access);
}

/** Puts a repository among a project's repositories (an owner). */
export function addProjectRepository(project: number, repository: number): Promise<Project> {
  return post<Project>(`/v1/projects/${project}/repositories`, { repository });
}

/** Takes a repository out of a project; its home project keeps it (an owner). */
export function removeProjectRepository(project: number, repository: number): Promise<Project> {
  return del<Project>(`/v1/projects/${project}/repositories/${repository}`);
}

/** Clones a repository into the server's data directory; the server makes its home project with it. */
export function addRepository(repository: NewRepository): Promise<Repository> {
  return post<Repository>("/v1/repositories", repository);
}

/** Replaces the logins whose issues need no approval (a repository on a forge). */
export function editTrusted(repository: number, trusted: string[]): Promise<Repository> {
  return put<Repository>(`/v1/repositories/${repository}`, { trusted });
}

/** What the server does with the repository's issues in agreement with other servers (`PUT /v1/repositories/{id}`). */
export interface WorkingWithOthers {
  mark_ready: boolean;
  ready_label: string;
  agents_take: AgentsTake;
  commands: boolean;
  command_limit: number;
  answer_on_forge: boolean;
  merge_reviewed: boolean;
}

/** Sets how the repository works with people on other servers: the ready label and which issues its agents take. */
export function editWorkingWithOthers(
  repository: number,
  settings: WorkingWithOthers,
): Promise<Repository> {
  return put<Repository>(`/v1/repositories/${repository}`, settings);
}

/** Removes a repository with its cards, changes, chats and clone; the server refuses while one of its runs goes on. */
export function removeRepository(repository: number): Promise<Repository> {
  return del<Repository>(`/v1/repositories/${repository}`);
}

/** What each running plugin asks of the repository, with the values it has. */
export function pluginProjectSettings(repository: number): Promise<PluginProjectSettings> {
  return get<PluginProjectSettings>(`/v1/repositories/${repository}/plugin-settings`);
}

/** Replaces every value of one plugin's settings for the repository; an empty value is none. */
export function savePluginProjectSettings(
  repository: number,
  plugin: string,
  values: Record<string, string>,
): Promise<PluginSettings> {
  return put<PluginSettings>(
    `/v1/repositories/${repository}/plugin-settings/${encodeURIComponent(plugin)}`,
    { values },
  );
}

/** The path of a repository's file or directory in an address: each part is encoded, the slashes stay. */
function inPath(path: string): string {
  return path.split("/").map(encodeURIComponent).join("/");
}

/** A directory of the default branch; `""` is the root. */
export function tree(repository: number, directory: string): Promise<Tree> {
  return get<Tree>(`/v1/repositories/${repository}/tree?path=${encodeURIComponent(directory)}`);
}

/** The paths of every file of the default branch, for "Go to file". */
export async function filePaths(repository: number): Promise<string[]> {
  const { paths } = await get<{ paths: string[] }>(`/v1/repositories/${repository}/files`);
  return paths;
}

export function readFile(repository: number, path: string): Promise<FileDetail> {
  return get<FileDetail>(`/v1/repositories/${repository}/files/${inPath(path)}`);
}

/** Commits the file to the default branch. */
export function saveFile(repository: number, path: string, text: string): Promise<FileDetail> {
  return put<FileDetail>(`/v1/repositories/${repository}/files/${inPath(path)}`, { text });
}

/** A full-text search of the default branch. */
export function searchFiles(repository: number, query: string, regex: boolean): Promise<Search> {
  return get<Search>(
    `/v1/repositories/${repository}/search?q=${encodeURIComponent(query)}${regex ? "&regex=true" : ""}`,
  );
}

/** The settings of a repository's project: the files of `repos/<name>/` in the harness configuration repository. */
export function readSettings(repository: number): Promise<ProjectSettings> {
  return get<ProjectSettings>(`/v1/repositories/${repository}/settings`);
}

/** Saves one file of the settings, a commit in the harness configuration repository that the server checks first. */
export function saveSetting(repository: number, file: string, text: string): Promise<SettingFile> {
  return put<SettingFile>(`/v1/repositories/${repository}/settings/${file}`, { text });
}

/** Removes one file of the settings. */
export function removeSetting(repository: number, file: string): Promise<SettingFile> {
  return del<SettingFile>(`/v1/repositories/${repository}/settings/${file}`);
}
