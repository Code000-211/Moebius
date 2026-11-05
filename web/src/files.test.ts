/**
 * The web app is built and tested on Windows too, where names that differ only in case are one name. An import names a
 * module without its extension, so `./ChatList` beside `chatList.ts` and `ChatList.tsx` is ambiguous there: no two
 * files under `src/` and `e2e/` may name modules that differ only in case.
 */
import { readdirSync } from "node:fs";
import { join, resolve } from "node:path";
import { describe, expect, it } from "vitest";

function files(dir: string): string[] {
  return readdirSync(dir, { withFileTypes: true }).flatMap((entry) =>
    entry.isDirectory() ? files(join(dir, entry.name)) : [join(dir, entry.name)],
  );
}

describe("the file names", () => {
  it("never differ only in case", () => {
    const root = resolve(import.meta.dirname, "..");
    const all = [...files(join(root, "src")), ...files(join(root, "e2e"))];
    const seen = new Map<string, string>();
    const clashes: string[] = [];
    for (const file of all) {
      const module = file.replace(/\.[^./\\]+$/, "");
      const other = seen.get(module.toLowerCase());
      if (other !== undefined && other !== module) clashes.push(`${other} and ${module}`);
      seen.set(module.toLowerCase(), module);
    }
    expect(clashes).toEqual([]);
  });
});
