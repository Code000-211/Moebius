/** One meaning for each path segment: `/board/` is the old board address, kept only in the legacy table. */
import { readdirSync, readFileSync, statSync } from "node:fs";
import { join } from "node:path";
import { describe, expect, it } from "vitest";
import {
  boardPath,
  cardPath,
  repositoryBoardPath,
  repositoryPath,
  scriptPath,
  settingsPath,
} from "./pages";

/** Every file under `dir`, as a path. */
function files(dir: string): string[] {
  return readdirSync(dir).flatMap((name) => {
    const path = join(dir, name);
    return statSync(path).isDirectory() ? files(path) : [path];
  });
}

describe("the addresses of the app", () => {
  it("names /board/ as an address only in the legacy table and in tests", () => {
    const root = join(__dirname, "..");
    const offenders = files(root)
      .filter((path) => /\.(ts|tsx)$/.test(path) && !/\.test\.tsx?$/.test(path))
      .filter((path) => !path.endsWith(join("routes", "redirects.ts")))
      // An address in a string or a pattern: `"/board/…`, `` `/board/…``, `/^\/board`.
      .filter((path) => /["'`]\/board\b|\\\/board\b/.test(readFileSync(path, "utf8")));
    expect(offenders).toEqual([]);
  });

  it("builds every address from a project id with the helpers of pages.ts", () => {
    expect(boardPath(3)).toBe("/projects/3");
    expect(boardPath(3, 8)).toBe("/projects/3?repository=8");
    expect(cardPath(3, 5)).toBe("/projects/3?card=5");
    expect(cardPath(3, 5, 8)).toBe("/projects/3?repository=8&card=5");
    expect(repositoryPath(3, 8)).toBe("/projects/3/repositories/8");
    expect(scriptPath(3, "In progress")).toBe("/projects/3/settings/columns/In%20progress");
    expect(settingsPath(3)).toBe("/projects/3/settings");
    expect(settingsPath(3, "general")).toBe("/projects/3/settings");
    expect(settingsPath(3, "members")).toBe("/projects/3/settings/members");
    expect(repositoryBoardPath({ id: 8, home_project_id: 3 })).toBe("/projects/3?repository=8");
    expect(repositoryBoardPath({ id: 8, home_project_id: 3 }, 5)).toBe(
      "/projects/3?repository=8&card=5",
    );
  });
});
