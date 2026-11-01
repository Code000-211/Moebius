/** The calls about repositories: adding, the trusted logins, removing, and reading and writing their files. */
import { del, get, post, put } from "../http";
import type { Repository } from "../types/board";
import type { FileDetail, NewRepository, Search, Tree } from "../types/projects";

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
