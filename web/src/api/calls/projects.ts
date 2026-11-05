/**
 * The calls about repositories: adding, the trusted logins, removing, reading and writing their files, their settings, and
 * the settings plugins ask of them.
 */
import { del, get, post, put } from "../http";
import type { Repository } from "../types/board";
import type {
  FileDetail,
  NewRepository,
  PluginProjectSettings,
  PluginSettings,
  ProjectSettings,
  Search,
  SettingFile,
  Tree,
} from "../types/projects";

/** Clones a repository into the server's data directory. */
export function addRepository(repository: NewRepository): Promise<Repository> {
  return post<Repository>("/v1/repositories", repository);
}

/** Replaces the logins whose issues need no approval (a repository on a forge). */
export function editTrusted(repository: number, trusted: string[]): Promise<Repository> {
  return put<Repository>(`/v1/repositories/${repository}`, { trusted });
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
